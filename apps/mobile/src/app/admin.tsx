import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { router } from "expo-router";
import {
  ArrowLeft,
  Cpu,
  Film,
  KeyRound,
  Link2,
  RefreshCw,
  ShieldOff,
  Trash2,
  Tv,
  UserPlus,
} from "lucide-react-native";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { Alert, Pressable, ScrollView, Share, Text, View } from "react-native";
import {
  Badge,
  Button,
  COLORS,
  EmptyState,
  ErrorText,
  Field,
  Input,
  Loading,
  ProgressBar,
  Screen,
} from "../components/ui";
import { ApiError, api, call } from "../lib/api";
import { meQuery } from "../lib/queries";
import { ui } from "../lib/scale";
import { session } from "../lib/session";

/**
 * The administration on the phone: what an admin checks or starts from the couch —
 * scans, accounts and invitations, the encode queue, timeline thumbnails, the server.
 * New libraries stay on the web.
 */
export default function Admin() {
  const { t } = useTranslation();
  const { data: me, isPending } = useQuery(meQuery);
  return (
    <Screen>
      <ScrollView contentContainerClassName="w-full max-w-3xl gap-5 self-center p-4 pb-10">
        <View className="flex-row items-center gap-3">
          <Pressable
            testID="back"
            accessibilityRole="button"
            accessibilityLabel={t("common.back")}
            hitSlop={8}
            onPress={() => (router.canGoBack() ? router.back() : router.replace("/settings"))}
            className="size-10 items-center justify-center rounded-full bg-surface-2 active:opacity-70"
          >
            <ArrowLeft color={COLORS.fg} size={ui(20)} />
          </Pressable>
          <Text className="flex-1 text-2xl font-bold text-fg">{t("admin.title")}</Text>
        </View>
        {isPending ? (
          <Loading />
        ) : me?.role !== "admin" ? null : !me.totp_enabled ? (
          // The server refuses administration without 2FA; it's turned on from the web.
          <EmptyState title={t("admin.needs2fa")} />
        ) : (
          <>
            <Libraries />
            <Users />
            <Encodes />
            <Thumbnails />
            <System />
          </>
        )}
      </ScrollView>
    </Screen>
  );
}

function Section({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <View className="gap-3 rounded-card border border-line bg-surface p-4">
      <View className="gap-1">
        <Text className="text-lg font-semibold text-fg">{title}</Text>
        {hint ? <Text className="text-sm text-muted">{hint}</Text> : null}
      </View>
      {children}
    </View>
  );
}

function useErrorText() {
  const { t } = useTranslation();
  return (e: unknown) =>
    e instanceof ApiError
      ? t(`errors.${e.code}`, { defaultValue: e.message })
      : e
        ? t("errors.network")
        : null;
}

function Libraries() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const errorText = useErrorText();
  const libraries = useQuery({
    queryKey: ["admin", "libraries"],
    queryFn: () => call(api().GET("/api/admin/libraries")),
  });
  const scans = useQuery({
    queryKey: ["admin", "scans"],
    queryFn: () => call(api().GET("/api/admin/scans")),
    refetchInterval: (q) => (q.state.data?.some((s) => s.running) ? 1500 : false),
  });
  const scan = useMutation({
    mutationFn: (id: number) =>
      call(api().POST("/api/admin/libraries/{id}/scan", { params: { path: { id } } })),
    onSettled: () => qc.invalidateQueries({ queryKey: ["admin", "scans"] }),
  });
  return (
    <Section title={t("admin.libraries")} hint={t("admin.librariesHint")}>
      {libraries.data?.length === 0 ? (
        <Text className="text-muted">{t("admin.noLibraries")}</Text>
      ) : null}
      {libraries.data?.map((lib) => {
        const Icon = lib.kind === "movies" ? Film : Tv;
        const running = scans.data?.some((s) => s.library_id === lib.id && s.running);
        return (
          <View key={lib.id} className="flex-row items-center gap-3">
            <View className="size-10 items-center justify-center rounded-xl bg-surface-2">
              <Icon color={COLORS.accent} size={ui(20)} />
            </View>
            <View className="min-w-0 flex-1">
              <Text className="font-medium text-fg">{lib.name}</Text>
              <Text numberOfLines={1} className="text-xs text-faint">
                {running ? t("admin.scanning") : `/media/${lib.path}`}
              </Text>
            </View>
            <Button
              testID={`scan-${lib.id}`}
              loading={running || (scan.isPending && scan.variables === lib.id)}
              icon={<RefreshCw color={COLORS.fg} size={ui(16)} />}
              onPress={() => scan.mutate(lib.id)}
            >
              {t("admin.scan")}
            </Button>
          </View>
        );
      })}
      <ErrorText>{errorText(scan.error)}</ErrorText>
    </Section>
  );
}

/** Same on every language. */
const TWO_FACTOR = "2FA";

/** Accounts (D106): create one, or share an invitation link; lock-out help. */
function Users() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const errorText = useErrorText();
  const { data: me } = useQuery(meQuery);
  const users = useQuery({
    queryKey: ["admin", "users"],
    queryFn: () => call(api().GET("/api/admin/users")),
  });
  const refresh = () => qc.invalidateQueries({ queryKey: ["admin", "users"] });
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState<"user" | "admin">("user");
  const [temporary, setTemporary] = useState<{ name: string; password: string } | null>(null);
  const create = useMutation({
    mutationFn: () =>
      call(api().POST("/api/admin/users", { body: { username: username.trim(), password, role } })),
    onSuccess: () => {
      setUsername("");
      setPassword("");
      setRole("user");
      void refresh();
    },
  });
  const remove = useMutation({
    mutationFn: (id: number) =>
      call(api().DELETE("/api/admin/users/{id}", { params: { path: { id } } })),
    onSuccess: refresh,
  });
  const resetPassword = useMutation({
    mutationFn: (u: { id: number; username: string }) =>
      call(api().POST("/api/admin/users/{id}/password", { params: { path: { id: u.id } } })),
    onSuccess: (data, u) => setTemporary({ name: u.username, password: data.password }),
  });
  const resetTotp = useMutation({
    mutationFn: (id: number) =>
      call(api().DELETE("/api/admin/users/{id}/totp", { params: { path: { id } } })),
    onSuccess: refresh,
  });
  const invite = useMutation({
    mutationFn: () => call(api().POST("/api/admin/invitations", { body: { role: "user" } })),
    onSuccess: (inv) =>
      void Share.share({ message: `${session().server}/invite/${inv.token}` }).catch(() => {}),
  });
  // Asks first: these can't be undone.
  const confirm = (message: string, action: string, run: () => void) =>
    Alert.alert(message, undefined, [
      { text: t("common.cancel"), style: "cancel" },
      { text: action, style: "destructive", onPress: run },
    ]);
  const error = create.error ?? remove.error ?? resetPassword.error ?? resetTotp.error;
  return (
    <Section title={t("admin.users")}>
      {users.data?.map((u) => (
        <View key={u.id} testID={`user-${u.username}`} className="flex-row items-center gap-3">
          <View className="size-9 items-center justify-center rounded-full bg-surface-3">
            <Text className="text-sm font-bold uppercase text-fg">{u.username.slice(0, 1)}</Text>
          </View>
          <View className="min-w-0 flex-1 gap-1">
            <Text numberOfLines={1} className="font-medium text-fg">
              {u.username}
            </Text>
            {u.role === "admin" || u.totp_enabled ? (
              <View className="flex-row gap-1.5">
                {u.role === "admin" ? <Badge tone="accent">{t("account.roleAdmin")}</Badge> : null}
                {u.totp_enabled ? <Badge tone="success">{TWO_FACTOR}</Badge> : null}
              </View>
            ) : null}
          </View>
          {u.id !== me?.id && u.totp_enabled ? (
            <IconButton
              label={`${t("admin.resetTotp")} · ${u.username}`}
              onPress={() =>
                confirm(t("admin.confirmResetTotp", { name: u.username }), t("admin.reset"), () =>
                  resetTotp.mutate(u.id),
                )
              }
            >
              <ShieldOff color={COLORS.muted} size={ui(18)} />
            </IconButton>
          ) : null}
          {u.id !== me?.id ? (
            <IconButton
              label={`${t("admin.resetPassword")} · ${u.username}`}
              onPress={() =>
                confirm(
                  t("admin.confirmResetPassword", { name: u.username }),
                  t("admin.reset"),
                  () => resetPassword.mutate(u),
                )
              }
            >
              <KeyRound color={COLORS.muted} size={ui(18)} />
            </IconButton>
          ) : null}
          {u.id !== me?.id ? (
            <IconButton
              testID={`remove-user-${u.username}`}
              label={`${t("common.delete")} ${u.username}`}
              onPress={() =>
                confirm(
                  t("admin.confirmRemoveUser", { name: u.username }),
                  t("common.delete"),
                  () => remove.mutate(u.id),
                )
              }
            >
              <Trash2 color={COLORS.muted} size={ui(18)} />
            </IconButton>
          ) : null}
        </View>
      ))}
      {temporary ? (
        // Shown once: selectable, and shareable to send it on.
        <View className="gap-2 rounded-xl bg-surface-2 p-3">
          <Text className="text-sm font-medium text-fg">{t("admin.temporaryPassword")}</Text>
          <Text className="text-xs text-muted">
            {t("admin.temporaryPasswordHint", { name: temporary.name })}
          </Text>
          <Text selectable testID="temporary-password" className="font-mono text-base text-fg">
            {temporary.password}
          </Text>
          <View className="flex-row gap-2">
            <Button
              onPress={() => void Share.share({ message: temporary.password }).catch(() => {})}
            >
              {t("common.share")}
            </Button>
            <Button onPress={() => setTemporary(null)}>{t("common.close")}</Button>
          </View>
        </View>
      ) : null}
      <ErrorText>{errorText(error)}</ErrorText>
      <View className="gap-3 border-t border-line pt-4">
        <Field label={t("auth.username")}>
          <Input
            testID="new-username"
            value={username}
            onChangeText={setUsername}
            autoCapitalize="none"
            autoCorrect={false}
          />
        </Field>
        <Field label={t("auth.password")}>
          <Input
            testID="new-password"
            value={password}
            onChangeText={setPassword}
            secureTextEntry
            autoCapitalize="none"
            autoComplete="new-password"
          />
        </Field>
        <View className="flex-row self-start rounded-full bg-surface-2 p-1">
          {(["user", "admin"] as const).map((r) => (
            <Pressable
              key={r}
              testID={`new-role-${r}`}
              accessibilityRole="button"
              accessibilityState={{ selected: role === r }}
              onPress={() => setRole(r)}
              className={`rounded-full px-3 py-1.5 ${role === r ? "bg-accent" : ""}`}
            >
              <Text
                className={`text-xs font-semibold ${role === r ? "text-accent-ink" : "text-muted"}`}
              >
                {t(r === "admin" ? "account.roleAdmin" : "account.roleUser")}
              </Text>
            </Pressable>
          ))}
        </View>
        <Button
          testID="create-user"
          variant="primary"
          loading={create.isPending}
          disabled={!username.trim() || password.length < 10}
          icon={<UserPlus color={COLORS.accentInk} size={ui(16)} />}
          onPress={() => create.mutate()}
        >
          {t("admin.createUser")}
        </Button>
        <Text className="text-xs text-faint">{t("admin.passwordRule")}</Text>
        <Button
          testID="share-invitation"
          loading={invite.isPending}
          icon={<Link2 color={COLORS.fg} size={ui(16)} />}
          onPress={() => invite.mutate()}
        >
          {t("admin.shareInvitation")}
        </Button>
        <ErrorText>{errorText(invite.error)}</ErrorText>
      </View>
    </Section>
  );
}

function IconButton({
  label,
  onPress,
  children,
  testID,
}: {
  label: string;
  onPress: () => void;
  children: ReactNode;
  testID?: string;
}) {
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={label}
      hitSlop={6}
      onPress={onPress}
      className="size-9 items-center justify-center rounded-full active:opacity-70"
    >
      {children}
    </Pressable>
  );
}

function Encodes() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { data } = useQuery({
    queryKey: ["admin", "encodes"],
    queryFn: () => call(api().GET("/api/admin/encodes")),
    refetchInterval: (q) =>
      q.state.data?.variants.some((v) => v.status === "queued" || v.status === "encoding")
        ? 3000
        : false,
  });
  const remove = useMutation({
    mutationFn: (id: number) =>
      call(api().DELETE("/api/admin/encodes/{id}", { params: { path: { id } } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["admin", "encodes"] }),
  });
  return (
    <Section title={t("admin.encodes")} hint={t("admin.encodesHint")}>
      {data?.variants.length ? (
        data.variants.map((v) => (
          <View key={v.id} className="flex-row items-center gap-3">
            <View className="min-w-0 flex-1 gap-1">
              <Text numberOfLines={1} className="text-sm text-fg">
                {v.file.split("/").pop()}
              </Text>
              <Text className="text-xs text-faint">
                {t(`downloads.profile.${v.profile}`)} · {v.codec.toUpperCase()}
              </Text>
              {v.status === "encoding" ? <ProgressBar value={v.progress} /> : null}
              {v.error ? <Text className="text-xs text-danger">{v.error}</Text> : null}
            </View>
            <Badge
              tone={
                v.status === "ready" ? "success" : v.status === "encoding" ? "accent" : "neutral"
              }
            >
              {t(`admin.encodeStatus.${v.status}`)}
            </Badge>
            <Pressable
              accessibilityRole="button"
              accessibilityLabel={t("downloads.remove")}
              hitSlop={8}
              onPress={() => remove.mutate(v.id)}
              className="size-9 items-center justify-center rounded-full active:opacity-70"
            >
              <Trash2 color={COLORS.muted} size={ui(18)} />
            </Pressable>
          </View>
        ))
      ) : data ? (
        <Text className="text-muted">{t("admin.noEncodes")}</Text>
      ) : null}
    </Section>
  );
}

/** Timeline thumbnails (D99): the missing ones in batches, like on the web. */
function Thumbnails() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const errorText = useErrorText();
  const { data } = useQuery({
    queryKey: ["admin", "trickplay"],
    queryFn: () => call(api().GET("/api/admin/trickplay")),
    refetchInterval: (q) => (q.state.data?.waiting || q.state.data?.current ? 3000 : false),
  });
  const [size, setSize] = useState("50");
  const refresh = () => qc.invalidateQueries({ queryKey: ["admin", "trickplay"] });
  const generate = useMutation({
    mutationFn: (count: number | null) =>
      call(api().POST("/api/admin/trickplay/generate", { body: { count } })),
    onSuccess: refresh,
  });
  const stop = useMutation({
    mutationFn: () => call(api().DELETE("/api/admin/trickplay/queue")),
    onSuccess: refresh,
  });
  if (!data) return null;
  const batch = Math.min(Math.max(1, Math.floor(Number(size) || 0)), data.missing);
  const busy = data.waiting > 0 || !!data.current;
  return (
    <Section title={t("admin.trickplay")} hint={t("admin.trickplayHint")}>
      <View className="gap-1.5">
        <Text className="text-xs text-muted">
          {t("admin.trickplayReady", { count: data.ready, videos: data.videos })}
          {data.unavailable
            ? ` · ${t("admin.trickplayUnavailable", { count: data.unavailable })}`
            : ""}
        </Text>
        <ProgressBar value={data.videos ? data.ready / data.videos : 0} />
      </View>
      {busy ? (
        <View className="flex-row items-center gap-3">
          <View className="min-w-0 flex-1">
            {data.current ? (
              <Text numberOfLines={1} className="text-xs text-fg">
                {t("admin.trickplayCurrent", { file: data.current.split("/").pop() })}
              </Text>
            ) : null}
            <Text className="text-xs text-faint">
              {t("admin.trickplayWaiting", { count: data.waiting })}
            </Text>
          </View>
          {data.waiting ? (
            <Button testID="trickplay-stop" onPress={() => stop.mutate()}>
              {t("admin.trickplayStop")}
            </Button>
          ) : null}
        </View>
      ) : null}
      {data.missing ? (
        <View className="gap-3">
          <Field label={t("admin.trickplayBatch")}>
            <Input
              testID="trickplay-batch"
              keyboardType="number-pad"
              value={size}
              onChangeText={setSize}
            />
          </Field>
          <Button
            testID="trickplay-generate"
            variant="primary"
            loading={generate.isPending && generate.variables !== null}
            onPress={() => generate.mutate(batch)}
          >
            {t("admin.trickplayGenerate", { count: batch })}
          </Button>
          <Button
            testID="trickplay-all"
            loading={generate.isPending && generate.variables === null}
            onPress={() => generate.mutate(null)}
          >
            {t("admin.trickplayAll", { count: data.missing })}
          </Button>
        </View>
      ) : !busy ? (
        <Text className="text-muted">{t("admin.trickplayDone")}</Text>
      ) : null}
      <ErrorText>{errorText(generate.error ?? stop.error)}</ErrorText>
    </Section>
  );
}

function System() {
  const { t } = useTranslation();
  const { data } = useQuery({
    queryKey: ["admin", "system"],
    queryFn: () => call(api().GET("/api/system/info")),
  });
  if (!data) return null;
  const hardware = (data.ffmpeg?.working_encoders ?? []).some((e) => e.includes("nvenc"));
  return (
    <Section title={t("admin.system")}>
      <View className="flex-row justify-between">
        <Text className="text-muted">{t("admin.version")}</Text>
        <Text className="text-fg">{data.version}</Text>
      </View>
      <View className="flex-row justify-between gap-4">
        <Text className="text-muted">ffmpeg</Text>
        <Text numberOfLines={1} className="flex-1 text-right text-fg">
          {data.ffmpeg ? (data.ffmpeg.version ?? t("admin.ffmpegMissing")) : t("admin.detecting")}
        </Text>
      </View>
      <View className="flex-row items-center gap-2">
        <Cpu color={hardware ? COLORS.success : COLORS.faint} size={ui(16)} />
        <Text className="flex-1 text-sm text-fg">
          {data.ffmpeg?.gpu_filters
            ? t("admin.gpuFull")
            : hardware
              ? t("admin.gpuOk")
              : t("admin.gpuMissing")}
        </Text>
      </View>
    </Section>
  );
}
