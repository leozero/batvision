import { formatBytes } from "@batvision/core/bytes";
import { episodeCode } from "@batvision/core/format";
import { useQuery } from "@tanstack/react-query";
import { router } from "expo-router";
import {
  CheckCircle2,
  Clock,
  Loader2,
  Pause,
  Play,
  RefreshCw,
  WifiOff,
  XCircle,
} from "lucide-react-native";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Pressable, ScrollView, Switch, Text, View } from "react-native";
import {
  Badge,
  Button,
  COLORS,
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

function ThisPhone() {
  const { t, i18n } = useTranslation();
  const state = useSyncState();
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
            </Pressable>
          ))}
        </View>
      ) : (
        <Muted>{t("downloads.nothingExpected")}</Muted>
      )}
    </View>
  );
}

function Choice<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <View className="flex-row flex-wrap gap-2">
      {options.map((o) => (
        <Pressable
          key={o.value}
          testID={`choice-${o.value}`}
          onPress={() => onChange(o.value)}
          className={`rounded-full px-3 py-2 ${o.value === value ? "bg-accent" : "bg-surface-2"}`}
        >
          <Text
            className={`text-xs font-semibold ${o.value === value ? "text-accent-ink" : "text-muted"}`}
          >
            {o.label}
          </Text>
        </Pressable>
      ))}
    </View>
  );
}

function Toggle({
  label,
  value,
  onChange,
  testID,
}: {
  label: string;
  value: boolean;
  onChange: (v: boolean) => void;
  testID?: string;
}) {
  return (
    <View className="flex-row items-center justify-between gap-4 rounded-xl bg-surface-2 px-4 py-3">
      <Text className="flex-1 text-sm font-medium text-fg">{label}</Text>
      <Switch
        testID={testID}
        value={value}
        onValueChange={onChange}
        trackColor={{ true: COLORS.accent }}
      />
    </View>
  );
}

/** This phone's settings, stored on the server (D76) so the web can change them too. */
function Settings({ device }: { device: Schemas["DeviceDownloads"] }) {
  const { t } = useTranslation();
  const sync = useSyncState();
  const save = useSaveSettings(device.id);
  const [s, setS] = useState(device.settings);
  useEffect(() => setS(device.settings), [device.settings]);
  const update = (patch: Partial<Schemas["DeviceSettings"]>) => {
    const next = { ...s, ...patch };
    setS(next);
    save.mutate(next);
  };
  const gb = (bytes: number | null | undefined) => (bytes ? String(Math.round(bytes / GB)) : "");
  return (
    <View className="gap-4 rounded-card border border-line bg-surface p-4">
      <Text className="text-lg font-semibold text-fg">{t("mobile.settings")}</Text>
      <Toggle
        testID="auto-download"
        label={t("downloads.auto")}
        value={s.auto_download}
        onChange={(v) => update({ auto_download: v })}
      />
      <Field label={t("downloads.quality")}>
        <Choice
          value={s.profile}
          onChange={(profile) => update({ profile })}
          options={(["max", "mobile", "saver"] as const).map((p) => ({
            value: p,
            label: t(`downloads.profile.${p}`),
          }))}
        />
      </Field>
      <Field label={t("downloads.quota")}>
        <Input
          testID="quota"
          keyboardType="number-pad"
          defaultValue={gb(s.quota_bytes)}
          placeholder="∞"
          onEndEditing={(e) =>
            update({ quota_bytes: e.nativeEvent.text ? Number(e.nativeEvent.text) * GB : null })
          }
        />
      </Field>
      <Field label={t("downloads.mobileData")}>
        <Choice
          value={s.mobile_data}
          onChange={(mobile_data) => update({ mobile_data })}
          options={(["off", "light", "all"] as const).map((m) => ({
            value: m,
            label: t(`downloads.mobileDataMode.${m}`),
          }))}
        />
      </Field>
      {s.mobile_data !== "off" ? (
        <Field label={t("downloads.mobileCap")}>
          <Input
            testID="mobile-cap"
            keyboardType="number-pad"
            defaultValue={gb(s.mobile_cap_bytes)}
            placeholder="2"
            onEndEditing={(e) =>
              update({
                mobile_cap_bytes: e.nativeEvent.text ? Number(e.nativeEvent.text) * GB : null,
              })
            }
          />
        </Field>
      ) : null}
      <Toggle
        label={t("downloads.chargingOnly")}
        value={s.charging_only}
        onChange={(v) => update({ charging_only: v })}
      />
      <Field label={t("downloads.storage")}>
        <Choice
          value={s.storage}
          onChange={(storage) => update({ storage })}
          options={[
            { value: "internal" as const, label: t("mobile.storagePhone") },
            { value: "sd" as const, label: t("mobile.storageSd") },
          ]}
        />
        {s.storage === "sd" && !sync.sdAvailable ? <Muted>{t("mobile.sdMissing")}</Muted> : null}
      </Field>
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
