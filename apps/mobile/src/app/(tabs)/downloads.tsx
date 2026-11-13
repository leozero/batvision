import { formatBytes } from "@batvision/core/bytes";
import { episodeCode } from "@batvision/core/format";
import { useQueryClient } from "@tanstack/react-query";
import { router } from "expo-router";
import {
  CheckCircle2,
  Clock,
  HardDriveDownload,
  Loader2,
  MoreVertical,
  Pause,
  Play,
  RefreshCw,
  Settings,
  WifiOff,
  XCircle,
} from "lucide-react-native";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { Pressable, ScrollView, Text, View } from "react-native";
import { LocalLibrary } from "../../components/LocalLibrary";
import { Sheet } from "../../components/Sheet";
import {
  Badge,
  Button,
  COLORS,
  EmptyState,
  ErrorText,
  Muted,
  Poster,
  ProgressBar,
  Screen,
} from "../../components/ui";
import { BatvisionSync, type SyncItem, useSyncState } from "../../lib/sync";

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

/** Sync status, space used and the transfers still to come, then the library. */
function ThisDevice() {
  const { t, i18n } = useTranslation();
  const state = useSyncState();
  const [action, setAction] = useState<SyncItem | null>(null);
  const used = formatBytes(state.usedBytes, i18n.language);
  const pending = state.items.filter((i) => i.status !== "done");
  const done = state.items.filter((i) => i.status === "done");
  return (
    <View className="gap-6">
      <View className="gap-3 rounded-card border border-line bg-surface p-4">
        <View className="flex-row flex-wrap items-center gap-2">
          <Text className="flex-1 text-sm text-muted">
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
          </Text>
          {state.online ? null : (
            <Badge tone="accent">
              <WifiOff color={COLORS.accent} size={12} />
              <Text className="text-xs font-semibold text-accent">
                {t("desktop.serverUnreachable")}
              </Text>
            </Badge>
          )}
        </View>
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
      </View>

      {pending.length ? (
        <View className="gap-3">
          <Text className="text-lg font-semibold text-fg">{t("mobile.inProgress")}</Text>
          {pending.map((item) => (
            <View
              key={item.key}
              testID={`local-${item.key}`}
              className="flex-row items-center gap-3 rounded-xl bg-surface p-2"
            >
              <Poster uri={item.meta.poster} className="w-12" />
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
            </View>
          ))}
        </View>
      ) : null}

      {done.length ? (
        <LocalLibrary
          items={done}
          testPrefix="local"
          onPlay={(item) => router.push(`/watch/${item.kind}/${item.id}`)}
          onMore={setAction}
        />
      ) : null}
      {state.items.length === 0 ? (
        <EmptyState
          icon={<HardDriveDownload color={COLORS.faint} size={32} />}
          title={t("mobile.offlineEmpty")}
        >
          <Muted>{t("downloads.nothingExpected")}</Muted>
        </EmptyState>
      ) : null}
      <ItemActions item={action} onClose={() => setAction(null)} />
    </View>
  );
}

export default function Downloads() {
  const { t } = useTranslation();
  return (
    <Screen>
      <ScrollView contentContainerClassName="w-full max-w-5xl gap-5 self-center p-4 pb-10">
        <View className="flex-row items-center justify-between">
          <Text className="text-2xl font-bold text-fg">{t("nav.downloads")}</Text>
          <Pressable
            testID="downloads-settings"
            accessibilityRole="button"
            accessibilityLabel={t("mobile.settings")}
            hitSlop={8}
            onPress={() => router.push("/settings")}
            className="size-10 items-center justify-center rounded-full bg-surface-2 active:opacity-70"
          >
            <Settings color={COLORS.fg} size={20} />
          </Pressable>
        </View>
        <ThisDevice />
      </ScrollView>
    </Screen>
  );
}
