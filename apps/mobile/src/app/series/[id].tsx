import { episodeCode } from "@batvision/core/format";
import { useQuery } from "@tanstack/react-query";
import { Image } from "expo-image";
import { router, useLocalSearchParams } from "expo-router";
import { Download, Play } from "lucide-react-native";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Pressable, ScrollView, Text, View } from "react-native";
import { Hero, LocalMark, WatchedMark } from "../../components/Detail";
import {
  Button,
  COLORS,
  LoadError,
  Loading,
  Muted,
  ProgressBar,
  Rating,
  Screen,
} from "../../components/ui";
import { SeriesWatchlistButton } from "../../components/Watchlist";
import { imageUrl, type Schemas } from "../../lib/api";
import { seasonQuery, seriesQuery, useManualDownload, useSubscribe } from "../../lib/queries";
import { session } from "../../lib/session";

const ROLLING = 5;

function Episode({ e }: { e: Schemas["EpisodeSummary"] }) {
  const { t } = useTranslation();
  const download = useManualDownload();
  const [requested, setRequested] = useState(false);
  const progress = e.duration_ms && e.position_ms && !e.played ? e.position_ms / e.duration_ms : 0;
  return (
    <View className="flex-row gap-3 py-3">
      <Pressable
        testID={`episode-${e.id}`}
        accessibilityRole="button"
        accessibilityLabel={`${t("common.play")} ${episodeCode(e.season_number, e.episode_number)}`}
        onPress={() => router.push(`/watch/episode/${e.id}`)}
        className="w-36 active:opacity-70"
      >
        <View className="aspect-video overflow-hidden rounded-lg bg-surface-2">
          {e.still_path ? (
            <Image
              source={{ uri: imageUrl(e.still_path, "w500") }}
              style={{ width: "100%", height: "100%" }}
              contentFit="cover"
            />
          ) : null}
          <View className="absolute inset-0 items-center justify-center">
            <Play color={COLORS.fg} size={22} fill={COLORS.fg} />
          </View>
        </View>
        {progress ? <ProgressBar value={progress} className="mt-1" /> : null}
      </Pressable>
      <View className="flex-1 gap-1">
        <Text className="text-xs font-semibold text-accent">
          {episodeCode(e.season_number, e.episode_number)}
        </Text>
        <Text numberOfLines={2} className="font-medium text-fg">
          {e.title ?? t("detail.episode", { number: e.episode_number })}
        </Text>
        <View className="flex-row flex-wrap items-center gap-2">
          <Rating ratings={e.ratings} />
          {e.played ? <WatchedMark /> : null}
          <LocalMark kind="episode" id={e.id} />
        </View>
        <Pressable
          testID={`download-episode-${e.id}`}
          accessibilityRole="button"
          accessibilityLabel={t("mobile.downloadHere")}
          hitSlop={8}
          disabled={requested}
          onPress={() => {
            const deviceId = session().deviceId;
            if (deviceId != null)
              download.mutate(
                { device_id: deviceId, kind: "episode", id: e.id },
                { onSuccess: () => setRequested(true) },
              );
          }}
          className="self-start"
        >
          <Download color={requested ? COLORS.faint : COLORS.muted} size={18} />
        </Pressable>
      </View>
    </View>
  );
}

export default function SeriesScreen() {
  const { t } = useTranslation();
  const id = Number(useLocalSearchParams<{ id: string }>().id);
  const series = useQuery(seriesQuery(id));
  const subscribe = useSubscribe();
  const [season, setSeason] = useState<number | null>(null);
  const [subscribed, setSubscribed] = useState<string | null>(null);
  const s = series.data;
  useEffect(() => {
    if (s && season === null)
      setSeason(
        s.up_next?.season_number ??
          s.seasons.find((x) => x.season_number > 0)?.season_number ??
          s.seasons[0]?.season_number ??
          1,
      );
  }, [s, season]);
  const episodes = useQuery({ ...seasonQuery(id, season ?? 1), enabled: season !== null });
  if (series.isError) return <LoadError onRetry={() => void series.refetch()} />;
  if (series.isPending || !s) return <Loading />;
  const playLabel = s.up_next
    ? `${t("common.play")} · ${episodeCode(s.up_next.season_number, s.up_next.episode_number)}`
    : "";
  const sub = (mode: "rolling" | "season") => {
    const deviceId = session().deviceId;
    if (deviceId == null || season === null) return;
    subscribe.mutate(
      {
        device_id: deviceId,
        series_id: id,
        mode,
        count: mode === "rolling" ? ROLLING : null,
        season_number: mode === "season" ? season : null,
      },
      { onSuccess: () => setSubscribed(mode) },
    );
  };
  return (
    <Screen edges={[]}>
      <ScrollView contentContainerClassName="pb-10">
        <Hero backdrop={imageUrl(s.backdrop_path ?? s.poster_path, "w1280")}>
          <Text testID="title" className="text-2xl font-bold text-fg">
            {s.title}
          </Text>
          <View className="flex-row flex-wrap items-center gap-3">
            {s.year ? <Muted>{String(s.year)}</Muted> : null}
            <Rating ratings={s.ratings} />
          </View>
        </Hero>
        <View className="w-full max-w-3xl gap-4 self-center p-4">
          {s.up_next ? (
            <Button
              testID="play"
              variant="primary"
              icon={<Play color={COLORS.accentInk} size={18} />}
              onPress={() => router.push(`/watch/episode/${s.up_next?.id}`)}
            >
              {playLabel}
            </Button>
          ) : null}
          <SeriesWatchlistButton series={s} />
          <View className="gap-2">
            <Text className="text-sm font-semibold text-muted">{t("mobile.downloadHere")}</Text>
            <View className="flex-row gap-2">
              <View className="flex-1">
                <Button
                  testID="subscribe-rolling"
                  disabled={subscribed === "rolling"}
                  loading={subscribe.isPending}
                  onPress={() => sub("rolling")}
                >
                  {t("mobile.nextEpisodes", { count: ROLLING })}
                </Button>
              </View>
              <View className="flex-1">
                <Button
                  testID="subscribe-season"
                  disabled={subscribed === "season"}
                  onPress={() => sub("season")}
                >
                  {t("mobile.wholeSeason", { number: season ?? 1 })}
                </Button>
              </View>
            </View>
            {subscribed ? <Muted>{t("mobile.requested")}</Muted> : null}
          </View>
          {s.overview ? (
            <Text numberOfLines={5} className="text-sm leading-5 text-muted">
              {s.overview}
            </Text>
          ) : null}
          <ScrollView
            horizontal
            showsHorizontalScrollIndicator={false}
            contentContainerClassName="gap-2"
          >
            {s.seasons.map((x) => (
              <Pressable
                key={x.season_number}
                onPress={() => setSeason(x.season_number)}
                className={`rounded-full px-3 py-2 ${x.season_number === season ? "bg-accent" : "bg-surface-2"}`}
              >
                <Text
                  className={`text-xs font-semibold ${x.season_number === season ? "text-accent-ink" : "text-muted"}`}
                >
                  {x.season_number === 0
                    ? t("detail.specials")
                    : t("detail.season", { number: x.season_number })}
                </Text>
              </Pressable>
            ))}
          </ScrollView>
          <View className="divide-y divide-line">
            {(episodes.data ?? []).map((e) => (
              <Episode key={e.id} e={e} />
            ))}
          </View>
        </View>
      </ScrollView>
    </Screen>
  );
}
