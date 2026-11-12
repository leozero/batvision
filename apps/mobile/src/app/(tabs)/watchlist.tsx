import { useQuery } from "@tanstack/react-query";
import { Link } from "expo-router";
import { DownloadCloud } from "lucide-react-native";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { FlatList, Pressable, RefreshControl, Text, useWindowDimensions, View } from "react-native";
import { ChoiceList, SettingRow, Sheet } from "../../components/Sheet";
import { Button, COLORS, EmptyState, Loading, Muted, Poster, Screen } from "../../components/ui";
import { imageUrl } from "../../lib/api";
import {
  downloadsQuery,
  preferencesQuery,
  useSavePreferences,
  useSaveSettings,
  watchlistQuery,
} from "../../lib/queries";
import { session } from "../../lib/session";

type Filter = "all" | "movie" | "series";

/** Automatic downloads off on this phone: the watchlist doesn't come here (D42). */
function AutoOffBanner() {
  const { t } = useTranslation();
  const overview = useQuery(downloadsQuery);
  const device = overview.data?.devices.find((d) => d.id === session().deviceId);
  const save = useSaveSettings(device?.id ?? 0);
  if (!device || device.settings.auto_download) return null;
  return (
    <View testID="auto-off" className="gap-3 rounded-card border border-accent/30 bg-accent/10 p-4">
      <View className="flex-row items-center gap-3">
        <DownloadCloud color={COLORS.accent} size={20} />
        <Text className="flex-1 text-sm text-fg">{t("watchlist.autoOffHere")}</Text>
      </View>
      <Button
        testID="enable-auto"
        variant="primary"
        loading={save.isPending}
        onPress={() => save.mutate({ ...device.settings, auto_download: true })}
      >
        {t("watchlist.enableHere")}
      </Button>
    </View>
  );
}

export default function WatchlistScreen() {
  const { t } = useTranslation();
  const list = useQuery(watchlistQuery);
  const prefs = useQuery(preferencesQuery);
  const savePrefs = useSavePreferences();
  const [filter, setFilter] = useState<Filter>("all");
  const [episodesOpen, setEpisodesOpen] = useState(false);
  const { width } = useWindowDimensions();
  const columns = Math.max(3, Math.floor(width / 150));
  if (list.isPending) return <Loading />;
  const items = (list.data ?? []).filter((i) => filter === "all" || i.kind === filter);
  const count = prefs.data?.watchlist_episodes ?? 2;
  const filters: { value: Filter; label: string }[] = [
    { value: "all", label: t("watchlist.all") },
    { value: "movie", label: t("watchlist.movies") },
    { value: "series", label: t("watchlist.series") },
  ];
  return (
    <Screen>
      <FlatList
        key={columns}
        data={items}
        numColumns={columns}
        keyExtractor={(i) => `${i.kind}-${i.id}`}
        refreshControl={
          <RefreshControl refreshing={list.isRefetching} onRefresh={() => void list.refetch()} />
        }
        contentContainerClassName="gap-4 px-3 pb-8"
        columnWrapperClassName="gap-3"
        ListHeaderComponent={
          <View className="gap-3 px-1 pt-4">
            <Text className="text-2xl font-bold text-fg">{t("watchlist.title")}</Text>
            <Muted>{t("watchlist.intro")}</Muted>
            <SettingRow
              testID="watchlist-episodes"
              label={t("watchlist.episodesSetting")}
              value={t("watchlist.episodes", { count })}
              onPress={() => setEpisodesOpen(true)}
            />
            <AutoOffBanner />
            <View className="flex-row gap-2">
              {filters.map((f) => (
                <Pressable
                  key={f.value}
                  testID={`filter-${f.value}`}
                  accessibilityState={{ selected: filter === f.value }}
                  onPress={() => setFilter(f.value)}
                  className={`rounded-full px-3 py-1.5 ${filter === f.value ? "bg-fg" : "bg-surface-2"}`}
                >
                  <Text
                    className={`text-xs font-semibold ${filter === f.value ? "text-bg" : "text-muted"}`}
                  >
                    {f.label}
                  </Text>
                </Pressable>
              ))}
            </View>
          </View>
        }
        ListEmptyComponent={<EmptyState title={t("watchlist.empty")} />}
        renderItem={({ item }) => (
          <Link
            href={(item.kind === "movie" ? `/movie/${item.id}` : `/series/${item.id}`) as never}
            asChild
          >
            <Pressable
              testID={`watchlist-${item.kind}-${item.id}`}
              style={{ width: `${100 / columns - 3}%` }}
              className="active:opacity-70"
            >
              <Poster uri={imageUrl(item.poster_path, "w342")} />
              <Text numberOfLines={1} className="mt-1.5 text-xs font-medium text-fg">
                {item.title}
              </Text>
              <Text className="text-xs text-faint">
                {item.played ? t("watchlist.watched") : item.year ? String(item.year) : " "}
              </Text>
            </Pressable>
          </Link>
        )}
      />
      <Sheet
        open={episodesOpen}
        onClose={() => setEpisodesOpen(false)}
        title={t("watchlist.episodesSetting")}
        description={t("account.watchlistEpisodesHint")}
      >
        <ChoiceList
          value={String(count)}
          options={Array.from({ length: 10 }, (_, i) => ({
            value: String(i + 1),
            label: t("downloads.nextEpisodes", { count: i + 1 }),
          }))}
          onChange={(v) => {
            if (prefs.data) savePrefs.mutate({ ...prefs.data, watchlist_episodes: Number(v) });
            setEpisodesOpen(false);
          }}
        />
      </Sheet>
    </Screen>
  );
}
