import { useQueryClient } from "@tanstack/react-query";
import { router } from "expo-router";
import { HardDriveDownload, RefreshCw, WifiOff } from "lucide-react-native";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ScrollView, Text, View } from "react-native";
import { LocalLibrary } from "../components/LocalLibrary";
import { Badge, Button, COLORS, EmptyState, Muted, Screen } from "../components/ui";
import { meQuery } from "../lib/queries";
import { ui } from "../lib/scale";
import { type SyncItem, useSyncState } from "../lib/sync";
import { IS_TV } from "../lib/tv";

const watch = (item: SyncItem) =>
  router.push({
    pathname: "/watch/[kind]/[id]",
    params: { kind: item.kind, id: String(item.id), offline: "1" },
  });

/** The server can't be reached: what's on this phone, playable as is (D72). */
export default function Offline() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const state = useSyncState();
  const [retrying, setRetrying] = useState(false);
  const done = state.items.filter((i) => i.status === "done");

  const retry = async () => {
    setRetrying(true);
    qc.removeQueries({ queryKey: meQuery.queryKey });
    try {
      await qc.fetchQuery(meQuery);
      router.replace("/home");
    } catch {
      // Still unreachable.
    } finally {
      setRetrying(false);
    }
  };

  return (
    <Screen>
      <ScrollView contentContainerClassName="w-full max-w-5xl gap-6 self-center p-4 pb-10">
        <View className="flex-row items-center gap-3">
          <Text className="text-2xl font-bold text-accent">BatVision</Text>
          <Badge tone="accent">
            <WifiOff color={COLORS.accent} size={ui(12)} />
            <Text className="text-xs font-semibold text-accent">{t("desktop.offlineTitle")}</Text>
          </Badge>
        </View>
        <Button
          testID="retry"
          tvFocus
          loading={retrying}
          onPress={retry}
          icon={<RefreshCw color={COLORS.fg} size={ui(16)} />}
        >
          {t("desktop.retry")}
        </Button>
        {done.length === 0 ? (
          <EmptyState
            icon={<HardDriveDownload color={COLORS.faint} size={ui(32)} />}
            // A TV keeps nothing: only the server can help (D110).
            title={IS_TV ? t("tv.offlineEmpty") : t("mobile.offlineEmpty")}
          />
        ) : null}
        <LocalLibrary items={done} testPrefix="offline" onPlay={watch} />
        {state.items.some((i) => i.status !== "done") ? (
          <Muted>{t("downloads.nothingExpected")}</Muted>
        ) : null}
      </ScrollView>
    </Screen>
  );
}
