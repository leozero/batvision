import { formatBytes } from "@batvision/core/bytes";
import { episodeCode } from "@batvision/core/format";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { router } from "expo-router";
import {
  CheckCircle2,
  Clock,
  Loader2,
  MoreVertical,
  Pause,
  Play,
  RefreshCw,
  WifiOff,
  XCircle,
} from "lucide-react-native";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Pressable, ScrollView, Text, View } from "react-native";
import { type Choice, ChoiceList, SettingRow, Sheet, SwitchRow } from "../../components/Sheet";
import {
  Badge,
  Button,
  COLORS,
  ErrorText,
  Field,
  Input,
  Muted,
  ProgressBar,
  Screen,
} from "../../components/ui";
import { api, type Schemas } from "../../lib/api";
import { downloadsQuery, useSaveSettings } from "../../lib/queries";
import { session } from "../../lib/session";
import { BatvisionSync, type SyncItem, useSyncState } from "../../lib/sync";

const GB = 1_000_000_000;

function Line({
  icon,
  text,
  tone = "text-muted",
}: {
  icon: ReactNode;
  text: string;
  tone?: string;
}) {
  return (
    <View className="flex-row items-center gap-1.5">
      {icon}
      <Text className={`text-xs ${tone}`}>{text}</Text>
    </View>
  );
}

function ItemState({ item, paused }: { item: SyncItem; paused: boolean }) {
  const { t, i18n } = useTranslation();
  const size = 14;
  if (item.status === "done")
    return (
      <Line
        icon={<CheckCircle2 color={COLORS.success} size={size} />}
        text={t("desktop.state.done")}
        tone="text-success"
      />
    );
  if (item.overQuota)
    return (
      <Line
        icon={<XCircle color={COLORS.faint} size={size} />}
        text={t("desktop.state.overQuota")}
      />
    );
  if (item.status === "failed")
    return (
      <Line
        icon={<XCircle color={COLORS.danger} size={size} />}
        text={t("desktop.state.failed")}
        tone="text-danger"
      />
    );
  if (item.status === "downloading")
    return (
      <Line
        icon={<Loader2 color={COLORS.accent} size={size} />}
        text={t("desktop.state.downloading", {
          received: formatBytes(item.receivedBytes, i18n.language),
          total: formatBytes(item.totalBytes, i18n.language),
        })}
        tone="text-accent"
      />
    );
  if (item.serverStatus === "encoding")
    return (
      <Line
        icon={<Loader2 color={COLORS.accent} size={size} />}
        text={t("desktop.state.encoding", { percent: Math.round(item.encodeProgress * 100) })}
        tone="text-accent"
      />
    );
  if (item.serverStatus === "queued")
    return (
      <Line icon={<Clock color={COLORS.faint} size={size} />} text={t("desktop.state.queued")} />
    );
  if (paused && item.serverStatus === "ready")
    return (
      <Line icon={<Pause color={COLORS.faint} size={size} />} text={t("desktop.state.paused")} />
    );
  if (item.wait)
    return (
      <Line
        icon={<Clock color={COLORS.faint} size={size} />}
        text={t(`mobile.wait.${item.wait}`)}
      />
    );
  return (
    <Line icon={<Clock color={COLORS.faint} size={size} />} text={t("desktop.state.waiting")} />
  );
}

/** Cancels a download, or deletes a finished one, saying what goes with it (D74). */
function ItemActions({ item, onClose }: { item: SyncItem | null; onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const close = () => {
    setError(false);
    onClose();
  };
  const done = item?.status === "done";
  const run = async () => {
    if (!item) return;
    setBusy(true);
    setError(false);
    try {
      await BatvisionSync.cancel(item.kind, item.id);
      void qc.invalidateQueries();
      close();
    } catch {
      setError(true);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Sheet
      open={item !== null}
      onClose={close}
      testID="item-actions"
      title={done ? t("downloads.deleteTitle") : t("downloads.cancelTitle")}
      description={
        item ? t(`downloads.effect.${item.source ?? "manual"}`, { title: item.meta.title }) : ""
      }
    >
      <ErrorText>{error ? t("downloads.cancelFailed") : null}</ErrorText>
      <Button testID="confirm-cancel" variant="danger" loading={busy} onPress={() => void run()}>
        {done ? t("downloads.delete") : t("downloads.cancel")}
      </Button>
      <Button onPress={close}>{t("downloads.keep")}</Button>
    </Sheet>
  );
}

function ThisPhone() {
  const { t, i18n } = useTranslation();
  const state = useSyncState();
  const [action, setAction] = useState<SyncItem | null>(null);
  const used = formatBytes(state.usedBytes, i18n.language);
  return (
    <View className="gap-4 rounded-card border border-line bg-surface p-4">
      <View className="flex-row flex-wrap items-center gap-2">
        <Text className="text-lg font-semibold text-fg">{t("mobile.thisPhone")}</Text>
        {state.online ? null : (
          <Badge tone="accent">
            <WifiOff color={COLORS.accent} size={12} />
            <Text className="text-xs font-semibold text-accent">
              {t("desktop.serverUnreachable")}
            </Text>
          </Badge>
        )}
      </View>
      <Muted>
        {state.syncing
          ? t("desktop.syncing")
          : state.lastSyncAt
            ? t("desktop.lastSync", {
                time: new Date(state.lastSyncAt).toLocaleTimeString(i18n.language, {
                  hour: "2-digit",
                  minute: "2-digit",
                }),
              })
            : ""}
      </Muted>
      <View className="flex-row gap-2">
        <View className="flex-1">
          <Button
            testID="pause"
            onPress={() => BatvisionSync.setPaused(!state.paused)}
            icon={
              state.paused ? (
                <Play color={COLORS.fg} size={16} />
              ) : (
                <Pause color={COLORS.fg} size={16} />
              )
            }
          >
            {state.paused ? t("desktop.resume") : t("desktop.pause")}
          </Button>
        </View>
        <View className="flex-1">
          <Button
            testID="sync-now"
            loading={state.syncing}
            onPress={() => BatvisionSync.syncNow()}
            icon={<RefreshCw color={COLORS.fg} size={16} />}
          >
            {t("desktop.syncNow")}
          </Button>
        </View>
      </View>
      <View className="gap-2">
        <Text className="text-sm text-fg">
          {state.quotaBytes
            ? t("desktop.used", { used, quota: formatBytes(state.quotaBytes, i18n.language) })
            : t("desktop.usedNoQuota", { used })}
        </Text>
        {state.quotaBytes ? <ProgressBar value={state.usedBytes / state.quotaBytes} /> : null}
        <Muted>
          {state.mobileCapBytes
            ? t("mobile.mobileUsed", {
                used: formatBytes(state.mobileBytesThisMonth, i18n.language),
                cap: formatBytes(state.mobileCapBytes, i18n.language),
              })
            : t("mobile.mobileUsedNoCap", {
                used: formatBytes(state.mobileBytesThisMonth, i18n.language),
              })}
        </Muted>
      </View>
      {state.items.length ? (
        <View className="overflow-hidden rounded-xl border border-line">
          {state.items.map((item, i) => (
            <Pressable
              key={item.key}
              testID={`local-${item.key}`}
              disabled={item.status !== "done"}
              onPress={() => router.push(`/watch/${item.kind}/${item.id}`)}
              className={`flex-row items-center gap-3 px-3 py-3 ${i ? "border-t border-line" : ""}`}
            >
              <View className="flex-1 gap-1">
                <Text numberOfLines={1} className="text-sm font-medium text-fg">
                  {item.meta.title}
                  {item.kind === "episode" ? (
                    <Text className="text-muted">
                      {" "}
                      · {episodeCode(item.meta.seasonNumber, item.meta.episodeNumber)}
                    </Text>
                  ) : null}
                </Text>
                <ItemState item={item} paused={state.paused} />
                {item.status === "downloading" && item.totalBytes ? (
                  <ProgressBar value={item.receivedBytes / item.totalBytes} />
                ) : null}
              </View>
              {item.status === "done" ? <Play color={COLORS.fg} size={18} /> : null}
              <Pressable
                testID={`more-${item.key}`}
                accessibilityRole="button"
                accessibilityLabel={t("mobile.more")}
                hitSlop={8}
                onPress={() => setAction(item)}
                className="size-9 items-center justify-center rounded-full active:bg-surface-2"
              >
                <MoreVertical color={COLORS.muted} size={18} />
              </Pressable>
            </Pressable>
          ))}
        </View>
      ) : (
        <Muted>{t("downloads.nothingExpected")}</Muted>
      )}
      <ItemActions item={action} onClose={() => setAction(null)} />
    </View>
  );
}

type SettingSheet = "profile" | "quota" | "mobile_data" | "mobile_cap" | "storage" | null;

/** Space in GB: a few common values, none ("unlimited"), or one typed in. */
function GigabytesChoice({
  value,
  presets,
  noneLabel,
  onChange,
}: {
  value: number | null | undefined;
  presets: number[];
  noneLabel: string;
  onChange: (bytes: number | null) => void;
}) {
  const { t } = useTranslation();
  const current = value ? Math.round(value / GB) : null;
  const preset = current === null || presets.includes(current);
  const [custom, setCustom] = useState(preset ? "" : String(current));
  const options: Choice<string>[] = [
    ...presets.map((n) => ({ value: String(n), label: t("mobile.gigabytes", { count: n }) })),
    { value: "none", label: noneLabel },
  ];
  return (
    <View className="gap-3">
      <ChoiceList
        value={current === null ? "none" : preset ? String(current) : ""}
        options={options}
        onChange={(v) => onChange(v === "none" ? null : Number(v) * GB)}
      />
      <Field label={t("mobile.otherValue")}>
        <View className="flex-row gap-2">
          <View className="flex-1">
            <Input
              testID="gigabytes"
              keyboardType="number-pad"
              value={custom}
              placeholder={t("mobile.gigabytesInput")}
              onChangeText={setCustom}
            />
          </View>
          <Button
            testID="gigabytes-apply"
            variant="primary"
            disabled={!Number(custom)}
            onPress={() => onChange(Number(custom) * GB)}
          >
            {t("mobile.apply")}
          </Button>
        </View>
      </Field>
    </View>
  );
}

/** This phone's settings, stored on the server so the web can change them too. */
function Settings({ device }: { device: Schemas["DeviceDownloads"] }) {
  const { t, i18n } = useTranslation();
  const sync = useSyncState();
  const save = useSaveSettings(device.id);
  const [s, setS] = useState(device.settings);
  const [sheet, setSheet] = useState<SettingSheet>(null);
  useEffect(() => setS(device.settings), [device.settings]);
  const update = (patch: Partial<Schemas["DeviceSettings"]>) => {
    const next = { ...s, ...patch };
    setS(next);
    save.mutate(next);
    setSheet(null);
  };
  const size = (bytes: number | null | undefined, none: string) =>
    bytes ? formatBytes(bytes, i18n.language) : none;
  const profiles = (["max", "mobile", "saver"] as const).map((p) => ({
    value: p,
    label: t(`downloads.profile.${p}`),
    hint: t(`downloads.profileHint.${p}`),
  }));
  const mobileModes = (["off", "light", "all"] as const).map((m) => ({
    value: m,
    label: t(`downloads.mobileDataMode.${m}`),
    hint: t(`downloads.mobileDataHint.${m}`),
  }));
  const storages = [
    {
      value: "internal" as const,
      label: t("mobile.storagePhone"),
      hint: t("mobile.storageHint.internal"),
    },
    { value: "sd" as const, label: t("mobile.storageSd"), hint: t("mobile.storageHint.sd") },
  ];
  const close = () => setSheet(null);
  return (
    <View className="gap-3 rounded-card border border-line bg-surface p-4">
      <Text className="text-lg font-semibold text-fg">{t("mobile.settings")}</Text>
      <SwitchRow
        testID="auto-download"
        label={t("downloads.auto")}
        hint={t("downloads.autoHint")}
        value={s.auto_download}
        onChange={(v) => update({ auto_download: v })}
      />
      <SettingRow
        testID="setting-profile"
        label={t("downloads.quality")}
        value={t(`downloads.profile.${s.profile}`)}
        hint={t(`downloads.profileHint.${s.profile}`)}
        onPress={() => setSheet("profile")}
      />
      <SettingRow
        testID="setting-quota"
        label={t("mobile.quotaLabel")}
        value={size(s.quota_bytes, t("mobile.unlimited"))}
        hint={t("mobile.quotaHint")}
        onPress={() => setSheet("quota")}
      />
      <SettingRow
        testID="setting-mobile-data"
        label={t("downloads.mobileData")}
        value={t(`downloads.mobileDataMode.${s.mobile_data}`)}
        hint={t(`downloads.mobileDataHint.${s.mobile_data}`)}
        onPress={() => setSheet("mobile_data")}
      />
      {s.mobile_data !== "off" ? (
        <SettingRow
          testID="setting-mobile-cap"
          label={t("mobile.capLabel")}
          value={size(s.mobile_cap_bytes, t("mobile.noCap"))}
          hint={t("mobile.capHint")}
          onPress={() => setSheet("mobile_cap")}
        />
      ) : null}
      <SwitchRow
        label={t("downloads.chargingOnly")}
        hint={t("downloads.chargingOnlyHint")}
        value={s.charging_only}
        onChange={(v) => update({ charging_only: v })}
      />
      <SettingRow
        testID="setting-storage"
        label={t("downloads.storage")}
        value={s.storage === "sd" ? t("mobile.storageSd") : t("mobile.storagePhone")}
        hint={s.storage === "sd" && !sync.sdAvailable ? t("mobile.sdMissing") : undefined}
        onPress={() => setSheet("storage")}
      />

      <Sheet open={sheet === "profile"} onClose={close} title={t("downloads.quality")}>
        <ChoiceList
          value={s.profile}
          options={profiles}
          onChange={(profile) => update({ profile })}
        />
      </Sheet>
      <Sheet
        open={sheet === "quota"}
        onClose={close}
        title={t("mobile.quotaLabel")}
        description={t("mobile.quotaHint")}
      >
        <GigabytesChoice
          value={s.quota_bytes}
          presets={[10, 20, 50, 100]}
          noneLabel={t("mobile.unlimited")}
          onChange={(quota_bytes) => update({ quota_bytes })}
        />
      </Sheet>
      <Sheet open={sheet === "mobile_data"} onClose={close} title={t("downloads.mobileData")}>
        <ChoiceList
          value={s.mobile_data}
          options={mobileModes}
          onChange={(mobile_data) => update({ mobile_data })}
        />
      </Sheet>
      <Sheet
        open={sheet === "mobile_cap"}
        onClose={close}
        title={t("mobile.capLabel")}
        description={t("mobile.capHint")}
      >
        <GigabytesChoice
          value={s.mobile_cap_bytes}
          presets={[1, 2, 5, 10]}
          noneLabel={t("mobile.noCap")}
          onChange={(mobile_cap_bytes) => update({ mobile_cap_bytes })}
        />
      </Sheet>
      <Sheet open={sheet === "storage"} onClose={close} title={t("downloads.storage")}>
        <ChoiceList
          value={s.storage}
          options={storages}
          onChange={(storage) => update({ storage })}
        />
        {!sync.sdAvailable ? <Muted>{t("mobile.sdMissing")}</Muted> : null}
      </Sheet>
    </View>
  );
}

export default function Downloads() {
  const { t } = useTranslation();
  const overview = useQuery(downloadsQuery);
  const deviceId = session().deviceId;
  const device = overview.data?.devices.find((d) => d.id === deviceId);
  const signOut = async () => {
    await api()
      .POST("/api/auth/logout")
      .catch(() => {});
    BatvisionSync.signOut();
    router.replace("/login");
  };
  return (
    <Screen>
      <ScrollView contentContainerClassName="gap-5 p-4 pb-10">
        <Text className="text-2xl font-bold text-fg">{t("nav.downloads")}</Text>
        <ThisPhone />
        {device ? <Settings device={device} /> : null}
        <Button variant="danger" onPress={signOut}>
          {t("mobile.signOut")}
        </Button>
      </ScrollView>
    </Screen>
  );
}
