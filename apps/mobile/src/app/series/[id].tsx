import { episodeCode, formatRuntime, progressRatio } from "@batvision/core/format";
import { useQuery } from "@tanstack/react-query";
import { Image } from "expo-image";
import { router, useLocalSearchParams } from "expo-router";
import { Check, CheckCircle2, ChevronDown, Download, Play } from "lucide-react-native";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ScrollView, Text, View } from "react-native";
import { Hero, LocalMark } from "../../components/Detail";
import { Pressable, useFocusRing } from "../../components/Pressable";
import { ChoiceList, Sheet } from "../../components/Sheet";
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
import {
  seasonQuery,
  seriesQuery,
  useManualDownload,
  useSetPlayed,
  useSubscribe,
} from "../../lib/queries";
import { ui } from "../../lib/scale";
import { session } from "../../lib/session";
import { useLocalItem } from "../../lib/sync";
import { IS_TV, TV_EDGE, tvTail } from "../../lib/tv";

/** How many of the next episodes to keep on the phone: the obvious picks, or all of them (D108). */
const ROLLING = ["1", "3", "5", "10", "all"] as const;
type Rolling = (typeof ROLLING)[number];

/** Round icon button on the right of an episode: watched, download. */
function EpisodeAction({
  testID,
  label,
  active,
  disabled,
  onPress,
  children,
}: {
  testID: string;
  label: string;
  active?: boolean;
  disabled?: boolean;
  onPress: () => void;
  children: ReactNode;
}) {
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ selected: !!active, disabled: !!disabled }}
      hitSlop={4}
      disabled={disabled}
      onPress={onPress}
      className={`size-10 items-center justify-center rounded-full active:opacity-70 ${active ? "bg-success/15" : "bg-surface-2"}`}
    >
      {children}
    </Pressable>
  );
}

/** An episode's picture, with the ring when the remote is on it (D112). */
function Still({ children }: { children: ReactNode }) {
  const ring = useFocusRing();
  return (
    <View style={ring} className="aspect-video overflow-hidden rounded-lg bg-surface-2">
      {children}
    </View>
  );
}

function Episode({ e }: { e: Schemas["EpisodeSummary"] }) {
  const { t, i18n } = useTranslation();
  const download = useManualDownload();
  const setPlayed = useSetPlayed();
  const [requested, setRequested] = useState(false);
  const [expanded, setExpanded] = useState(false);
  // Already on this phone, or on its way: the mark under the title says so.
  const local = useLocalItem("episode", e.id);
  const duration = e.duration_ms ?? (e.runtime_minutes ? e.runtime_minutes * 60_000 : null);
  const progress = e.played ? 0 : progressRatio(e.position_ms, duration);
  const number = e.episode_number_end
    ? `${e.episode_number}-${e.episode_number_end}`
    : String(e.episode_number);
  const meta = [`${t("detail.episode", { number })}`, formatRuntime(duration, i18n.language)]
    .filter(Boolean)
    .join(" · ");
  return (
    <View className="gap-3 py-4">
      {/* On a TV, room between the ring around the still and the text (D112). */}
      <View className={`flex-row items-center ${IS_TV ? "gap-6" : "gap-3"}`}>
        <Pressable
          testID={`episode-${e.id}`}
          accessibilityRole="button"
          accessibilityLabel={`${t("common.play")} ${episodeCode(e.season_number, e.episode_number)}`}
          onPress={() => router.push(`/watch/episode/${e.id}`)}
          // Wider on a TV, seen from the couch.
          className={`${IS_TV ? "w-52" : "w-36"} active:opacity-70`}
          ringOnChild
        >
          <Still>
            {e.still_path ? (
              <Image
                source={{ uri: imageUrl(e.still_path, "w500") }}
                style={{ width: "100%", height: "100%", opacity: e.played ? 0.6 : 1 }}
                contentFit="cover"
              />
            ) : null}
            <View className="absolute inset-0 items-center justify-center">
              <View className="size-9 items-center justify-center rounded-full bg-black/50">
                <Play color={COLORS.fg} size={ui(16)} fill={COLORS.fg} />
              </View>
            </View>
            {e.played ? (
              <View className="absolute right-1.5 top-1.5 size-5 items-center justify-center rounded-full bg-accent">
                <Check color={COLORS.accentInk} size={ui(12)} strokeWidth={3} />
              </View>
            ) : null}
            {progress ? (
              <ProgressBar value={progress} className="absolute inset-x-1.5 bottom-1.5" />
            ) : null}
          </Still>
        </Pressable>
        <View className="flex-1 gap-1">
          <Text numberOfLines={1} className="text-xs text-faint">
            {meta}
          </Text>
          <Text numberOfLines={2} className="font-semibold leading-5 text-fg">
            {e.title ?? t("detail.episode", { number })}
          </Text>
          <View className="flex-row flex-wrap items-center gap-2">
            <Rating ratings={e.ratings} />
            <LocalMark kind="episode" id={e.id} />
          </View>
        </View>
        <View className="gap-2">
          <EpisodeAction
            testID={`played-episode-${e.id}`}
            label={e.played ? t("detail.markUnwatched") : t("detail.markWatched")}
            active={e.played}
            disabled={setPlayed.isPending}
            onPress={() => setPlayed.mutate({ kind: "episode", id: e.id, played: !e.played })}
          >
            <Check color={e.played ? COLORS.success : COLORS.muted} size={ui(18)} />
          </EpisodeAction>
          {/* A TV streams only (D110). */}
          {local || IS_TV ? null : (
            <EpisodeAction
              testID={`download-episode-${e.id}`}
              label={t("mobile.downloadHere")}
              disabled={requested}
              onPress={() => {
                const deviceId = session().deviceId;
                if (deviceId != null)
                  download.mutate(
                    { device_id: deviceId, kind: "episode", id: e.id },
                    { onSuccess: () => setRequested(true) },
                  );
              }}
            >
              <Download color={requested ? COLORS.faint : COLORS.muted} size={ui(18)} />
            </EpisodeAction>
          )}
        </View>
      </View>
      {e.overview ? (
        // Three lines, the whole summary on a tap.
        <Pressable
          testID={`overview-episode-${e.id}`}
          accessibilityRole="button"
          accessibilityState={{ expanded }}
          onPress={() => setExpanded((x) => !x)}
          className="rounded-md"
        >
          <Text numberOfLines={expanded ? undefined : 3} className="text-sm leading-5 text-muted">
            {e.overview}
          </Text>
        </Pressable>
      ) : null}
    </View>
  );
}

export default function SeriesScreen() {
  const { t } = useTranslation();
  const id = Number(useLocalSearchParams<{ id: string }>().id);
  const series = useQuery(seriesQuery(id));
  const subscribe = useSubscribe();
  const setPlayed = useSetPlayed();
  const [season, setSeason] = useState<number | null>(null);
  const [subscribed, setSubscribed] = useState<string | null>(null);
  const [rolling, setRolling] = useState<Rolling>("5");
  const [picking, setPicking] = useState(false);
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
  // Nothing left to watch once every episode is: the server then has no next one.
  const allPlayed = s.seasons.length > 0 && !s.up_next;
  const current = s.seasons.find((x) => x.season_number === season);
  // Until the episodes are known, "watched" or "unwatched" would be a guess.
  const seasonPlayed = episodes.data ? episodes.data.every((e) => e.played) : null;
  const playLabel = s.up_next
    ? `${t("common.play")} · ${episodeCode(s.up_next.season_number, s.up_next.episode_number)}`
    : "";
  const rollingLabel = (r: Rolling) =>
    r === "all"
      ? t("downloads.allNextEpisodes")
      : t("downloads.nextEpisodes", { count: Number(r) });
  const sub = (mode: "rolling" | "season", count: Rolling = rolling) => {
    const deviceId = session().deviceId;
    if (deviceId == null || season === null) return;
    subscribe.mutate(
      {
        device_id: deviceId,
        series_id: id,
        mode,
        count: mode === "rolling" && count !== "all" ? Number(count) : null,
        all: mode === "rolling" && count === "all",
        season_number: mode === "season" ? season : null,
      },
      { onSuccess: () => setSubscribed(mode) },
    );
  };
  return (
    <Screen edges={[]}>
      <ScrollView fadingEdgeLength={TV_EDGE} contentContainerClassName={tvTail("pb-10")}>
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
          {/* On a TV, side by side, Play first (D112). */}
          <View className={IS_TV ? "flex-row flex-wrap gap-3" : "gap-4"}>
            {s.up_next ? (
              <Button
                testID="play"
                variant="primary"
                tvFocus
                icon={<Play color={COLORS.accentInk} size={ui(18)} />}
                onPress={() => router.push(`/watch/episode/${s.up_next?.id}`)}
              >
                {playLabel}
              </Button>
            ) : null}
            <SeriesWatchlistButton series={s} />
            <Button
              testID="played-series"
              icon={<Check color={COLORS.fg} size={ui(18)} />}
              loading={setPlayed.isPending && setPlayed.variables?.kind === "series"}
              onPress={() => setPlayed.mutate({ kind: "series", id, played: !allPlayed })}
            >
              {allPlayed ? t("detail.markUnwatched") : t("detail.markSeriesWatched")}
            </Button>
          </View>
          {/* A TV streams only (D110). */}
          {IS_TV ? null : (
            <>
              <View className="gap-2">
                <Text className="text-sm font-semibold text-muted">{t("mobile.downloadHere")}</Text>
                <View className="flex-row gap-2">
                  <View className="flex-1">
                    <Button
                      testID="subscribe-rolling"
                      loading={subscribe.isPending && subscribe.variables?.mode === "rolling"}
                      onPress={() => setPicking(true)}
                    >
                      <View className="flex-row items-center gap-1">
                        <Text numberOfLines={1} className="shrink font-semibold text-fg">
                          {rollingLabel(rolling)}
                        </Text>
                        <ChevronDown color={COLORS.muted} size={ui(16)} />
                      </View>
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
              <Sheet
                testID="rolling-sheet"
                open={picking}
                onClose={() => setPicking(false)}
                title={t("mobile.downloadHere")}
                description={t("downloads.rollingHint")}
              >
                <ChoiceList
                  value={subscribed === "rolling" ? rolling : ""}
                  options={ROLLING.map((r) => ({ value: r, label: rollingLabel(r) }))}
                  onChange={(r) => {
                    // "": nothing picked yet, never an option.
                    if (r === "") return;
                    setRolling(r);
                    setPicking(false);
                    sub("rolling", r);
                  }}
                />
              </Sheet>
            </>
          )}
          {s.overview ? (
            <Text numberOfLines={5} className="text-sm leading-5 text-muted">
              {s.overview}
            </Text>
          ) : null}
          <ScrollView
            horizontal
            showsHorizontalScrollIndicator={false}
            // Room for the ring around the season the remote is on (D112), the first season
            // still lined up with the page.
            className={IS_TV ? "-mx-2" : undefined}
            contentContainerClassName={`gap-2 ${IS_TV ? "px-2 py-2" : ""}`}
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
          {current ? (
            <Pressable
              testID="played-season"
              accessibilityRole="button"
              hitSlop={8}
              disabled={seasonPlayed === null || setPlayed.isPending}
              onPress={() =>
                setPlayed.mutate({ kind: "season", id: current.id, played: !seasonPlayed })
              }
              className={`flex-row items-center gap-2 self-start rounded-md active:opacity-70 ${seasonPlayed === null ? "opacity-50" : ""}`}
            >
              <CheckCircle2 color={seasonPlayed ? COLORS.success : COLORS.muted} size={ui(16)} />
              <Text className="text-sm font-semibold text-muted">
                {seasonPlayed ? t("detail.markSeasonUnwatched") : t("detail.markSeasonWatched")}
              </Text>
            </Pressable>
          ) : null}
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
