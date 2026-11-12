import { episodeCode } from "@batvision/core/format";
import { useQueryClient } from "@tanstack/react-query";
import { router } from "expo-router";
import { HardDriveDownload, Play, RefreshCw, WifiOff } from "lucide-react-native";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Pressable, ScrollView, Text, View } from "react-native";
import { Badge, Button, COLORS, EmptyState, Muted, Poster, Screen } from "../components/ui";
import { meQuery } from "../lib/queries";
import { type SyncItem, useSyncState } from "../lib/sync";

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
  const movies = done.filter((i) => i.kind === "movie");
  const series = useMemo(() => {
    const groups = new Map<
      string,
      { title: string; poster: string | null; episodes: SyncItem[] }
    >();
    for (const item of done.filter((i) => i.kind === "episode")) {
      const key = String(item.meta.seriesId ?? item.meta.title);
      const group = groups.get(key) ?? {
        title: item.meta.title,
        poster: item.meta.poster,
        episodes: [],
      };
      group.episodes.push(item);
      groups.set(key, group);
    }
    const order = (s: number | null) => (s === 0 ? 1e6 : (s ?? 0));
    for (const g of groups.values())
      g.episodes.sort(
        (a, b) =>
          order(a.meta.seasonNumber) - order(b.meta.seasonNumber) ||
          (a.meta.episodeNumber ?? 0) - (b.meta.episodeNumber ?? 0),
      );
    return [...groups.values()].sort((a, b) => a.title.localeCompare(b.title));
  }, [done]);

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
      <ScrollView contentContainerClassName="gap-6 p-4 pb-10">
        <View className="flex-row items-center gap-3">
          <Text className="text-2xl font-bold text-accent">batvision</Text>
          <Badge tone="accent">
            <WifiOff color={COLORS.accent} size={12} />
            <Text className="text-xs font-semibold text-accent">{t("desktop.offlineTitle")}</Text>
          </Badge>
        </View>
        <Button
          testID="retry"
          loading={retrying}
          onPress={retry}
          icon={<RefreshCw color={COLORS.fg} size={16} />}
        >
          {t("desktop.retry")}
        </Button>
        {done.length === 0 ? (
          <EmptyState
            icon={<HardDriveDownload color={COLORS.faint} size={32} />}
            title={t("mobile.offlineEmpty")}
          />
        ) : null}
        {movies.length ? (
          <View className="gap-3">
            <Text className="text-lg font-semibold text-fg">{t("desktop.movies")}</Text>
            <View className="flex-row flex-wrap gap-3">
              {movies.map((item) => (
                <Pressable
                  key={item.key}
                  testID={`offline-${item.key}`}
                  onPress={() => watch(item)}
                  className="w-[30%] active:opacity-70"
                >
                  <Poster uri={item.meta.poster} />
                  <Text numberOfLines={1} className="mt-1.5 text-xs font-medium text-fg">
                    {item.meta.title}
                  </Text>
                </Pressable>
              ))}
            </View>
          </View>
        ) : null}
        {series.map((group) => (
          <View key={group.title} className="gap-2">
            <Text className="text-lg font-semibold text-fg">{group.title}</Text>
            {group.episodes.map((item) => (
              <Pressable
                key={item.key}
                testID={`offline-${item.key}`}
                onPress={() => watch(item)}
                className="flex-row items-center gap-3 rounded-xl bg-surface p-3 active:opacity-70"
              >
                <Play color={COLORS.fg} size={16} />
                <View className="flex-1">
                  <Text className="text-xs font-semibold text-accent">
                    {episodeCode(item.meta.seasonNumber, item.meta.episodeNumber)}
                  </Text>
                  <Text numberOfLines={1} className="text-sm text-fg">
                    {item.meta.episodeTitle ?? item.meta.title}
                  </Text>
                </View>
              </Pressable>
            ))}
          </View>
        ))}
        {state.items.some((i) => i.status !== "done") ? (
          <Muted>{t("downloads.nothingExpected")}</Muted>
        ) : null}
      </ScrollView>
    </Screen>
  );
}
