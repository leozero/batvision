import { formatClock, languageName, sameLanguage } from "@batvision/core/format";
import { type Cue, cueAt, parseWebVtt } from "@batvision/core/subtitles";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { File } from "expo-file-system";
import { useKeepAwake } from "expo-keep-awake";
import * as NavigationBar from "expo-navigation-bar";
import { router, useFocusEffect, useLocalSearchParams } from "expo-router";
import * as ScreenOrientation from "expo-screen-orientation";
import { StatusBar } from "expo-status-bar";
import {
  ArrowLeft,
  Languages,
  Pause,
  Play,
  RotateCcw,
  RotateCw,
  Settings2,
  SkipForward,
} from "lucide-react-native";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ActivityIndicator,
  Modal,
  Platform,
  Pressable,
  ScrollView,
  Text,
  View,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import Video, {
  type ISO639_1,
  SelectedTrackType,
  TextTrackType,
  type VideoRef,
} from "react-native-video";
import { COLORS } from "../../../components/ui";
import { ApiError, api, call, type Schemas, Unreachable } from "../../../lib/api";
import {
  type AutoInput,
  autoStep,
  type Quality,
  type QualityMode,
  readAutoLevel,
  readQualityMode,
  saveAutoLevel,
  saveQualityMode,
} from "../../../lib/quality";
import { episodeQuery, movieQuery } from "../../../lib/queries";
import { session } from "../../../lib/session";
import { BatvisionSync } from "../../../lib/sync";

type Kind = "movie" | "episode";
type Info = Schemas["PlaybackInfo"];
type Local = NonNullable<ReturnType<typeof BatvisionSync.localPlayback>>;
type LocalAudio = { index: number; language: string | null; title: string | null };

const PROGRESS_EVERY_MS = 10_000;
const AUTO_EVERY_MS = 2_000;
/** Watched from 90 % (D36). */
const isPlayed = (positionMs: number, durationMs: number | null | undefined) =>
  Boolean(durationMs && positionMs >= durationMs * 0.9);

const absolute = (url: string) =>
  url.startsWith("http") || url.startsWith("file:") ? url : `${session().server}${url}`;

export default function Player() {
  useKeepAwake();
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const params = useLocalSearchParams<{ kind: Kind; id: string; offline?: string }>();
  const kind: Kind = params.kind === "episode" ? "episode" : "movie";
  const id = Number(params.id);
  const offline = params.offline === "1";
  const local: Local | null = useMemo(() => BatvisionSync.localPlayback(kind, id), [kind, id]);

  const online = !local && !offline;
  const movie = useQuery({ ...movieQuery(id), enabled: online && kind === "movie" });
  const episode = useQuery({ ...episodeQuery(id), enabled: online && kind === "episode" });
  const video = useRef<VideoRef>(null);
  const [info, setInfo] = useState<Info | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [paused, setPaused] = useState(false);
  const [loading, setLoading] = useState(true);
  // Only re-renders the clock and the bar: the position itself is in `positionRef`.
  const [, setTime] = useState(0);
  const [seekable, setSeekable] = useState(0);
  const [controls, setControls] = useState(true);
  // iPadOS keeps its status bar and window controls over the top corners.
  const insets = useSafeAreaInsets();
  const [sheet, setSheet] = useState<"tracks" | "quality" | null>(null);
  // A downloaded copy holds every audio track (D40), listed by the player once loaded;
  // its tracks switch in place.
  const [localAudio, setLocalAudio] = useState<LocalAudio[]>([]);
  const [localAudioIndex, setLocalAudioIndex] = useState<number | null>(null);
  const [localSubtitle, setLocalSubtitle] = useState<number | null>(() =>
    local?.subtitles.length ? 0 : null,
  );
  // iOS rebuilds a video with side-loaded subtitles keeping only its first audio track:
  // a downloaded copy's subtitles are drawn here instead.
  const drawSubtitles = Platform.OS === "ios" && local !== null;
  const [cues, setCues] = useState<Cue[]>([]);
  const subtitleUrl =
    drawSubtitles && localSubtitle !== null ? local?.subtitles[localSubtitle]?.url : undefined;
  useEffect(() => {
    setCues([]);
    if (!subtitleUrl) return;
    let current = true;
    new File(subtitleUrl)
      .text()
      .then((vtt) => current && setCues(parseWebVtt(vtt)))
      .catch(() => {});
    return () => {
      current = false;
    };
  }, [subtitleUrl]);
  const [mode, setMode] = useState<QualityMode>("auto");
  const modeLoaded = useRef(false);
  const [ended, setEnded] = useState(false);
  const positionRef = useRef(0); // absolute ms
  const barWidth = useRef(0);
  const infoRef = useRef<Info | null>(null);
  infoRef.current = info;

  // Auto quality inputs (D60).
  const auto = useRef({
    estimate: null as number | null,
    stalls: [] as number[],
    stallingSince: null as number | null,
    loadingSince: Date.now() as number | null,
    stableSince: Date.now(),
    lastUpAt: 0,
  });

  // Landscape, full screen. Not on iPad: iPadOS leaves rotation to the user, and a lock
  // it doesn't apply leaves the screen upright with its touches turned.
  useEffect(() => {
    const lock = !(Platform.OS === "ios" && Platform.isPad);
    if (lock) void ScreenOrientation.lockAsync(ScreenOrientation.OrientationLock.LANDSCAPE);
    void NavigationBar.setVisibilityAsync("hidden").catch(() => {});
    return () => {
      if (lock) void ScreenOrientation.unlockAsync();
      void NavigationBar.setVisibilityAsync("visible").catch(() => {});
    };
  }, []);

  const offset = info?.hls ? info.start_ms : 0;
  const duration = (local?.durationMs ?? info?.duration_ms ?? 0) / 1000;

  // ---- Session ---------------------------------------------------------------------------
  const start = useCallback(
    async (
      opts: {
        startMs?: number;
        audio?: number | null;
        subtitle?: string | null;
        quality?: Quality;
        remember?: boolean;
      } = {},
    ) => {
      setLoading(true);
      setError(null);
      setEnded(false);
      auto.current.loadingSince = Date.now();
      if (local || offline) {
        if (!local) setError(t("player.error"));
        return;
      }
      try {
        // The first start runs before the saved mode is in the state: read it here.
        const chosen = modeLoaded.current ? mode : await readQualityMode();
        const quality = opts.quality ?? (chosen === "auto" ? await readAutoLevel() : chosen);
        const next = await call(
          api().POST("/api/playback", {
            body: {
              kind,
              id,
              capabilities: BatvisionSync.capabilities(),
              start_ms: opts.startMs !== undefined ? Math.round(opts.startMs) : undefined,
              audio_index: opts.audio ?? undefined,
              subtitle: opts.subtitle === undefined ? undefined : (opts.subtitle ?? "off"),
              remember: opts.remember ?? false,
              quality,
            },
          }),
        );
        const previous = infoRef.current;
        if (previous?.session_id && previous.session_id !== next.session_id)
          void api().POST("/api/playback/stop", { body: { session_id: previous.session_id } });
        setInfo(next);
      } catch (e) {
        if (e instanceof Unreachable && local) return;
        setError(e instanceof ApiError ? e.message : t("player.error"));
        setLoading(false);
      }
    },
    [kind, id, local, offline, mode, t],
  );

  useEffect(() => {
    void readQualityMode().then((saved) => {
      modeLoaded.current = true;
      setMode(saved);
    });
  }, []);

  // Start once; later restarts come from the controls.
  // biome-ignore lint/correctness/useExhaustiveDependencies: run on mount only
  useEffect(() => {
    void start();
  }, []);

  // ---- Progress --------------------------------------------------------------------------
  const report = useCallback(() => {
    const position = Math.round(positionRef.current);
    if (position <= 0) return;
    if (local) {
      BatvisionSync.reportProgress(kind, id, position, isPlayed(position, local.durationMs));
      return;
    }
    void api()
      .POST("/api/progress", { body: { kind, id, position_ms: position } })
      .catch(() => {});
  }, [kind, id, local]);

  useEffect(() => {
    if (paused) return;
    const timer = setInterval(report, PROGRESS_EVERY_MS);
    return () => clearInterval(timer);
  }, [paused, report]);

  useEffect(
    () => () => {
      report();
      const last = infoRef.current;
      if (last?.session_id)
        void api()
          .POST("/api/playback/stop", { body: { session_id: last.session_id } })
          .catch(() => {});
      void qc.invalidateQueries();
    },
    [report, qc],
  );

  // ---- Auto quality ----------------------------------------------------------------------
  useEffect(() => {
    if (mode !== "auto" || !info || local) return;
    const timer = setInterval(() => {
      const a = auto.current;
      const input: AutoInput = {
        current: info.quality,
        options: info.qualities,
        estimateKbps: a.estimate,
        stalls: a.stalls,
        stallingSince: a.stallingSince,
        loadingSince: a.loadingSince,
        now: Date.now(),
        stableSince: a.stableSince,
        lastUpAt: a.lastUpAt,
      };
      const next = autoStep(input);
      if (!next || next === info.quality) return;
      const up =
        info.qualities.findIndex((q) => q.quality === next) <
        info.qualities.findIndex((q) => q.quality === info.quality);
      if (up) a.lastUpAt = Date.now();
      a.stalls = [];
      a.stableSince = Date.now();
      void saveAutoLevel(next);
      void start({ startMs: positionRef.current, quality: next });
    }, AUTO_EVERY_MS);
    return () => clearInterval(timer);
  }, [mode, info, local, start]);

  // ---- Controls --------------------------------------------------------------------------
  const seekTo = (absoluteMs: number) => {
    const target = Math.max(0, Math.min(absoluteMs, duration * 1000 || absoluteMs));
    const streamSeconds = (target - offset) / 1000;
    // Taps add up: the next +10 s starts from here, not from the last progress event.
    positionRef.current = target;
    setTime(streamSeconds);
    // Inside what's produced: seek; outside an HLS session: start a new one there.
    if (!info?.hls || (streamSeconds >= 0 && streamSeconds <= seekable))
      video.current?.seek(streamSeconds);
    else void start({ startMs: target });
  };

  // Leaving the player (back, another screen): stop and save now. The screen itself
  // may be torn down a few seconds later, and must not keep playing meanwhile.
  useFocusEffect(
    useCallback(
      () => () => {
        setPaused(true);
        report();
      },
      [report],
    ),
  );

  const goNext = () => {
    const nextId = local ? local.nextEpisodeId : info?.next_episode_id;
    if (!nextId) return;
    report();
    router.replace({
      pathname: "/watch/[kind]/[id]",
      params: { kind: "episode", id: String(nextId), ...(offline ? { offline: "1" } : {}) },
    });
  };

  // A new object on every render would make the player reload the video at its start
  // position, losing seeks: rebuilt only when the stream itself changes.
  const source = useMemo(
    () =>
      local
        ? {
            uri: local.url,
            startPosition: local.positionMs,
            textTracks: (drawSubtitles ? [] : local.subtitles).map((s, i) => ({
              title: s.language ?? `sub ${i}`,
              language: (s.language ?? "und") as ISO639_1,
              type: TextTrackType.VTT,
              uri: s.url,
            })),
          }
        : info
          ? {
              uri: absolute(info.url),
              type: info.hls ? "m3u8" : undefined,
              startPosition: info.hls ? 0 : info.start_ms,
              // iOS: the subtitle is listed inside the HLS stream (D88).
              inStream: Boolean(info.hls && info.subtitle_url && Platform.OS === "ios"),
              textTracks:
                info.subtitle_url && !(info.hls && Platform.OS === "ios")
                  ? [
                      {
                        title: "sub",
                        language: "und" as ISO639_1,
                        type: TextTrackType.VTT,
                        uri: absolute(info.subtitle_url),
                      },
                    ]
                  : [],
            }
          : null,
    [local, info, drawSubtitles],
  );

  const title = local
    ? [local.meta.title, local.meta.episodeTitle].filter(Boolean).join(" · ")
    : kind === "movie"
      ? (movie.data?.title ?? "")
      : [episode.data?.series_title, episode.data?.title].filter(Boolean).join(" · ");
  const currentSubtitle = info?.subtitle && info.subtitle !== "off" ? info.subtitle : null;
  // The tracks menu, from the server's session or from the downloaded copy.
  const audioOptions: LocalAudio[] = local
    ? localAudio
    : (info?.audio_tracks ?? []).map((a) => ({
        index: a.index,
        language: a.language ?? null,
        title: a.title ?? null,
      }));
  const audioSelected = local ? (localAudioIndex ?? localAudio[0]?.index) : info?.audio_index;
  const subtitleOptions = local
    ? local.subtitles.map((s, i) => ({
        id: `l${i}`,
        language: s.language,
        title: null as string | null,
        kind: s.kind,
      }))
    : (info?.subtitles ?? []).map((s) => ({
        id: s.id,
        language: s.language ?? null,
        title: s.title ?? null,
        kind: s.kind as string,
      }));
  const subtitleSelected = local
    ? localSubtitle === null
      ? null
      : `l${localSubtitle}`
    : currentSubtitle;
  const languageLabel = (code: string | null | undefined) =>
    languageName(code, i18n.language) ?? t("player.unknownLanguage");
  const position = positionRef.current / 1000;
  const nextId = local ? local.nextEpisodeId : info?.next_episode_id;

  return (
    <View className="flex-1 bg-black">
      <StatusBar hidden />
      {source ? (
        <Video
          ref={video}
          source={source}
          style={{ position: "absolute", inset: 0 }}
          resizeMode="contain"
          paused={paused}
          controls={false}
          // Transcoded streams are growing HLS playlists: not a live broadcast.
          controlsStyles={{ hideLiveBadge: true }}
          reportBandwidth
          enterPictureInPictureOnLeave
          selectedAudioTrack={
            local && localAudioIndex !== null
              ? { type: SelectedTrackType.INDEX, value: localAudioIndex }
              : undefined
          }
          selectedTextTrack={
            local
              ? localSubtitle === null || drawSubtitles
                ? { type: SelectedTrackType.DISABLED }
                : { type: SelectedTrackType.INDEX, value: localSubtitle }
              : source.textTracks.length || ("inStream" in source && source.inStream)
                ? { type: SelectedTrackType.INDEX, value: 0 }
                : { type: SelectedTrackType.DISABLED }
          }
          // Drawn subtitles (iOS downloads) follow the clock: a quarter second keeps them in step.
          progressUpdateInterval={drawSubtitles ? 250 : 500}
          onLoad={(data) => {
            if (local) {
              const tracks = (data.audioTracks ?? []).map((a) => ({
                index: a.index,
                language: a.language || null,
                // ffmpeg's default handler name says nothing.
                title: a.title && a.title !== "SoundHandler" ? a.title : null,
              }));
              setLocalAudio(tracks);
              // The language asked for when downloading plays first.
              const preferred = tracks.find((a) => sameLanguage(a.language, local.audioLanguage));
              setLocalAudioIndex((chosen) => chosen ?? preferred?.index ?? null);
            }
            setLoading(false);
            auto.current.loadingSince = null;
            auto.current.stableSince = Date.now();
          }}
          onProgress={(p) => {
            setTime(p.currentTime);
            setSeekable(p.seekableDuration);
            positionRef.current = offset + p.currentTime * 1000;
          }}
          onBuffer={({ isBuffering }) => {
            const a = auto.current;
            if (isBuffering) {
              a.stallingSince = Date.now();
              a.stalls.push(Date.now());
            } else {
              a.stallingSince = null;
              a.stableSince = Date.now();
            }
            setLoading(isBuffering);
          }}
          onBandwidthUpdate={(b) => {
            auto.current.estimate = b.bitrate / 1000;
          }}
          onEnd={() => {
            setEnded(true);
            positionRef.current = duration * 1000;
            report();
          }}
          onError={() => setError(t("player.error"))}
        />
      ) : null}

      {drawSubtitles && cueAt(cues, position) ? (
        <View
          testID="subtitle-cue"
          pointerEvents="none"
          className="absolute inset-x-0 items-center px-8"
          style={{ bottom: insets.bottom + 32 }}
        >
          <Text className="rounded-md bg-black/60 px-2 py-1 text-center text-lg text-white">
            {cueAt(cues, position)}
          </Text>
        </View>
      ) : null}

      <Pressable
        testID="player-surface"
        className="absolute inset-0"
        onPress={() => setControls((c) => !c)}
      />

      {loading && !error ? (
        <View
          testID="player-loading"
          pointerEvents="none"
          className="absolute inset-0 items-center justify-center"
        >
          <ActivityIndicator size="large" color={COLORS.accent} />
        </View>
      ) : null}

      {error ? (
        <View className="absolute inset-0 items-center justify-center gap-4">
          <Text className="text-base text-fg">{error}</Text>
          <Pressable
            accessibilityRole="button"
            onPress={() => void start({ startMs: positionRef.current })}
            className="rounded-xl bg-surface-2 px-4 py-3"
          >
            <Text className="font-semibold text-fg">{t("player.retry")}</Text>
          </Pressable>
        </View>
      ) : null}

      {controls ? (
        <View
          pointerEvents="box-none"
          className="absolute inset-0 justify-between bg-black/40"
          style={{
            paddingTop: insets.top + 16,
            paddingBottom: insets.bottom + 16,
            paddingLeft: insets.left + 16,
            paddingRight: insets.right + 16,
          }}
        >
          <View className="flex-row items-center gap-3">
            <Pressable
              testID="player-back"
              accessibilityRole="button"
              accessibilityLabel={t("common.back")}
              onPress={() => {
                setPaused(true);
                router.back();
              }}
              className="size-10 items-center justify-center rounded-full bg-black/50"
            >
              <ArrowLeft color={COLORS.fg} size={20} />
            </Pressable>
            <Text numberOfLines={1} className="flex-1 text-base font-semibold text-fg">
              {title}
            </Text>
            {!local ? (
              <Pressable
                testID="quality-menu"
                accessibilityRole="button"
                accessibilityLabel={t("player.quality")}
                onPress={() => setSheet("quality")}
                className="size-10 items-center justify-center"
              >
                <Settings2 color={COLORS.fg} size={20} />
              </Pressable>
            ) : null}
            {local || info ? (
              <Pressable
                testID="tracks-menu"
                accessibilityRole="button"
                accessibilityLabel={t("player.tracks")}
                onPress={() => setSheet("tracks")}
                className="size-10 items-center justify-center"
              >
                <Languages color={COLORS.fg} size={20} />
              </Pressable>
            ) : null}
          </View>
          <View className="flex-row items-center justify-center gap-10">
            <Pressable
              testID="back-10"
              accessibilityRole="button"
              accessibilityLabel={t("player.back10")}
              onPress={() => seekTo(positionRef.current - 10_000)}
            >
              <RotateCcw color={COLORS.fg} size={28} />
            </Pressable>
            <Pressable
              testID="play-pause"
              accessibilityRole="button"
              accessibilityLabel={t(paused ? "player.play" : "player.pause")}
              onPress={() => setPaused((p) => !p)}
              className="size-16 items-center justify-center rounded-full bg-black/50"
            >
              {paused ? (
                <Play color={COLORS.fg} size={30} fill={COLORS.fg} />
              ) : (
                <Pause color={COLORS.fg} size={30} fill={COLORS.fg} />
              )}
            </Pressable>
            <Pressable
              testID="forward-10"
              accessibilityRole="button"
              accessibilityLabel={t("player.forward10")}
              onPress={() => seekTo(positionRef.current + 10_000)}
            >
              <RotateCw color={COLORS.fg} size={28} />
            </Pressable>
          </View>
          <View className="gap-2">
            {(ended || (duration && position > duration - 30)) && nextId ? (
              <Pressable
                accessibilityRole="button"
                onPress={goNext}
                className="flex-row items-center gap-2 self-end rounded-xl bg-accent px-4 py-2"
              >
                <SkipForward color={COLORS.accentInk} size={16} />
                <Text className="font-semibold text-accent-ink">{t("player.nextEpisode")}</Text>
              </Pressable>
            ) : null}
            <Pressable
              className="h-6 justify-center"
              onPress={(e) => {
                const { locationX } = e.nativeEvent;
                const width = barWidth.current;
                if (width > 0 && duration > 0) seekTo((locationX / width) * duration * 1000);
              }}
              onLayout={(e) => {
                barWidth.current = e.nativeEvent.layout.width;
              }}
            >
              <View className="h-1 overflow-hidden rounded-full bg-white/25">
                <View
                  className="h-full bg-accent"
                  style={{ width: `${duration ? Math.min(100, (position / duration) * 100) : 0}%` }}
                />
              </View>
            </Pressable>
            <View className="flex-row justify-between">
              <Text testID="player-time" className="text-xs text-fg">
                {formatClock(position)}
              </Text>
              <Text className="text-xs text-fg">{duration ? formatClock(duration) : ""}</Text>
            </View>
          </View>
        </View>
      ) : null}

      <Modal
        visible={sheet !== null}
        transparent
        animationType="fade"
        onRequestClose={() => setSheet(null)}
        supportedOrientations={["landscape"]}
      >
        {/* Neither the backdrop nor the sheet become one element (iOS would merge every
            choice into it): each choice stays its own button. */}
        <Pressable
          accessible={false}
          className="flex-1 items-end justify-center bg-black/60 p-6"
          onPress={() => setSheet(null)}
        >
          <Pressable
            accessible={false}
            className="max-h-full w-80 rounded-card bg-surface p-4"
            onPress={() => {}}
          >
            <ScrollView contentContainerClassName="gap-1">
              {sheet === "quality" && info ? (
                <>
                  <Text className="mb-2 text-base font-semibold text-fg">
                    {t("player.quality")}
                  </Text>
                  {(["auto", ...info.qualities.map((q) => q.quality)] as QualityMode[]).map((q) => {
                    const option = info.qualities.find((o) => o.quality === q);
                    const detail = option
                      ? option.original
                        ? t("player.original")
                        : `${option.height}p`
                      : "";
                    const selected = q === mode;
                    return (
                      <Pressable
                        key={q}
                        testID={`quality-${q}`}
                        onPress={() => {
                          setSheet(null);
                          setMode(q);
                          void saveQualityMode(q);
                          if (q !== "auto" && q !== info.quality)
                            void start({ startMs: positionRef.current, quality: q });
                        }}
                        className={`flex-row justify-between rounded-lg px-3 py-2.5 ${selected ? "bg-accent/15" : ""}`}
                      >
                        <Text className={selected ? "font-semibold text-accent" : "text-fg"}>
                          {t(`player.qualityLevel.${q}`)}
                        </Text>
                        <Text className="text-xs text-faint">{detail}</Text>
                      </Pressable>
                    );
                  })}
                </>
              ) : null}
              {sheet === "tracks" && (local || info) ? (
                <>
                  <Text className="mb-2 text-base font-semibold text-fg">{t("player.audio")}</Text>
                  {audioOptions.map((a) => (
                    <Pressable
                      key={a.index}
                      testID={`audio-${a.index}`}
                      onPress={() => {
                        setSheet(null);
                        if (local) setLocalAudioIndex(a.index);
                        else
                          void start({
                            startMs: positionRef.current,
                            audio: a.index,
                            remember: true,
                          });
                      }}
                      className={`rounded-lg px-3 py-2.5 ${a.index === audioSelected ? "bg-accent/15" : ""}`}
                    >
                      <Text
                        className={
                          a.index === audioSelected ? "font-semibold text-accent" : "text-fg"
                        }
                      >
                        {[languageLabel(a.language), a.title].filter(Boolean).join(" · ")}
                      </Text>
                    </Pressable>
                  ))}
                  <Text className="mb-2 mt-4 text-base font-semibold text-fg">
                    {t("player.subtitles")}
                  </Text>
                  {[
                    { id: null as string | null, label: t("player.off") },
                    ...subtitleOptions.map((s) => ({
                      id: s.id as string | null,
                      label: [
                        languageLabel(s.language),
                        s.title,
                        s.kind !== "normal"
                          ? t(`player.kind.${s.kind}`, { defaultValue: "" })
                          : null,
                      ]
                        .filter(Boolean)
                        .join(" · "),
                    })),
                  ].map((s) => (
                    <Pressable
                      key={s.id ?? "off"}
                      onPress={() => {
                        setSheet(null);
                        if (local) setLocalSubtitle(s.id === null ? null : Number(s.id.slice(1)));
                        else
                          void start({
                            startMs: positionRef.current,
                            subtitle: s.id,
                            remember: true,
                          });
                      }}
                      className={`rounded-lg px-3 py-2.5 ${subtitleSelected === s.id ? "bg-accent/15" : ""}`}
                    >
                      <Text
                        className={
                          subtitleSelected === s.id ? "font-semibold text-accent" : "text-fg"
                        }
                      >
                        {s.label}
                      </Text>
                    </Pressable>
                  ))}
                </>
              ) : null}
            </ScrollView>
          </Pressable>
        </Pressable>
      </Modal>
    </View>
  );
}
