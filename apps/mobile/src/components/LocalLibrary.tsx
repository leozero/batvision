import { episodeCode } from "@batvision/core/format";
import { MoreVertical, Play } from "lucide-react-native";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Pressable, Text, useWindowDimensions, View } from "react-native";
import type { SyncItem } from "../lib/sync";
import { Sheet } from "./Sheet";
import { COLORS, Poster } from "./ui";

type Group = { key: string; title: string; poster: string | null; episodes: SyncItem[] };

/** Downloaded episodes by show, in watching order (specials last). */
function groupSeries(items: SyncItem[]): Group[] {
  const groups = new Map<string, Group>();
  for (const item of items.filter((i) => i.kind === "episode")) {
    const key = String(item.meta.seriesId ?? item.meta.title);
    const group = groups.get(key) ?? {
      key,
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
}

function MoreButton({ testID, onPress }: { testID: string; onPress: () => void }) {
  const { t } = useTranslation();
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={t("mobile.more")}
      hitSlop={8}
      onPress={onPress}
      className="size-8 items-center justify-center rounded-full bg-black/60 active:opacity-70"
    >
      <MoreVertical color={COLORS.fg} size={16} />
    </Pressable>
  );
}

/**
 * What's downloaded on this device, as posters like the other pages: movies, then one
 * poster per show whose downloaded episodes open in a sheet. Shared by the Downloads tab
 * and the offline screen; `onMore` adds the cancel/delete menu.
 */
export function LocalLibrary({
  items,
  testPrefix,
  onPlay,
  onMore,
}: {
  items: SyncItem[];
  /** `local` or `offline`: `local-movie-12`… */
  testPrefix: string;
  onPlay: (item: SyncItem) => void;
  onMore?: (item: SyncItem) => void;
}) {
  const { t } = useTranslation();
  const { width } = useWindowDimensions();
  const columns = Math.max(3, Math.floor(width / 150));
  const tile = { width: `${100 / columns - 3}%` as const };
  const movies = items.filter((i) => i.kind === "movie");
  const series = useMemo(() => groupSeries(items), [items]);
  const [open, setOpen] = useState<Group | null>(null);
  // The sheet follows the live state: a deleted episode leaves it.
  const openGroup = open ? series.find((g) => g.key === open.key) : undefined;

  return (
    <View className="gap-6">
      {movies.length ? (
        <View className="gap-3">
          <Text className="text-lg font-semibold text-fg">{t("nav.movies")}</Text>
          <View className="flex-row flex-wrap gap-3">
            {movies.map((item) => (
              <Pressable
                key={item.key}
                testID={`${testPrefix}-${item.key}`}
                onPress={() => onPlay(item)}
                style={tile}
                className="active:opacity-70"
              >
                <Poster uri={item.meta.poster} />
                {onMore ? (
                  <View className="absolute right-1.5 top-1.5">
                    <MoreButton testID={`more-${item.key}`} onPress={() => onMore(item)} />
                  </View>
                ) : null}
                <Text numberOfLines={1} className="mt-1.5 text-xs font-medium text-fg">
                  {item.meta.title}
                </Text>
                {item.meta.year ? (
                  <Text className="text-xs text-faint">{item.meta.year}</Text>
                ) : null}
              </Pressable>
            ))}
          </View>
        </View>
      ) : null}
      {series.length ? (
        <View className="gap-3">
          <Text className="text-lg font-semibold text-fg">{t("nav.series")}</Text>
          <View className="flex-row flex-wrap gap-3">
            {series.map((group) => (
              <Pressable
                key={group.key}
                testID={`${testPrefix}-series-${group.key}`}
                onPress={() => setOpen(group)}
                style={tile}
                className="active:opacity-70"
              >
                <Poster uri={group.poster} />
                <Text numberOfLines={1} className="mt-1.5 text-xs font-medium text-fg">
                  {group.title}
                </Text>
                <Text className="text-xs text-faint">
                  {t("detail.episodeCount", { count: group.episodes.length })}
                </Text>
              </Pressable>
            ))}
          </View>
        </View>
      ) : null}
      <Sheet open={Boolean(openGroup)} onClose={() => setOpen(null)} title={openGroup?.title ?? ""}>
        <View className="gap-2">
          {openGroup?.episodes.map((item) => (
            <View key={item.key} className="flex-row items-center gap-2">
              <Pressable
                testID={`${testPrefix}-${item.key}`}
                onPress={() => {
                  setOpen(null);
                  onPlay(item);
                }}
                className="flex-1 flex-row items-center gap-3 rounded-xl bg-surface-2 p-3 active:opacity-70"
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
              {onMore ? (
                <MoreButton
                  testID={`more-${item.key}`}
                  onPress={() => {
                    setOpen(null);
                    onMore(item);
                  }}
                />
              ) : null}
            </View>
          ))}
        </View>
      </Sheet>
    </View>
  );
}
