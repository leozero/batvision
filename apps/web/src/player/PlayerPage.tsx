import type { LocalPlayback } from "@batvision/bridge";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  useNavigate,
  useParams,
  useRouter,
  useRouterState,
  useSearch,
} from "@tanstack/react-router";
import Hls from "hls.js";
import type { TFunction } from "i18next";
import {
  ArrowLeft,
  Check,
  Maximize,
  Minimize,
  Pause,
  Play,
  RotateCcw,
  RotateCw,
  SkipForward,
  Subtitles,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { cx, IconButton, Spinner } from "../components/ui";
import { ApiError, api, call, type Schemas } from "../lib/api";
import { desktop, localPlaybackQuery } from "../lib/desktop";
import { episodeCode, formatClock, languageName } from "../lib/format";
import { episodeQuery, invalidateLibrary, movieQuery } from "../lib/queries";
import { detectCapabilities } from "./capabilities";

type Info = Schemas["PlaybackInfo"];

/** Watched at 90 % (D36). */
const isPlayed = (positionMs: number, durationMs: number | null | undefined) =>
  !!durationMs && positionMs >= durationMs * 0.9;

/** A downloaded copy, described like a direct-play session. */
function localInfo(local: LocalPlayback, opts: StartOptions, restart?: boolean): Info {
  const forced = local.subtitles.findIndex((s) => s.kind === "forced");
  const chosen =
    opts.subtitle === undefined
      ? forced
      : opts.subtitle === null || opts.subtitle === "off"
        ? -1
        : Number(opts.subtitle.slice(1));
  const sub = chosen >= 0 ? local.subtitles[chosen] : undefined;
  return {
    method: "direct",
    hls: false,
    url: local.url,
    start_ms:
      opts.startMs !== undefined ? Math.round(opts.startMs) : restart ? 0 : local.positionMs,
    duration_ms: local.durationMs,
    audio_tracks: [{ index: 0, codec: "aac", kind: "normal", language: local.audioLanguage }],
    audio_index: 0,
    subtitles: local.subtitles.map((s, i) => ({
      id: `l${i}`,
      external: true,
      text: true,
      kind: s.kind as Schemas["TrackKind"],
      language: s.language,
    })),
    subtitle: sub ? `l${chosen}` : null,
    subtitle_url: sub?.url ?? null,
    next_episode_id: local.nextEpisodeId,
    session_id: null,
  };
}
type Kind = "movie" | "episode";
type StartOptions = {
  startMs?: number;
  audio?: number | null;
  subtitle?: string | null;
  remember?: boolean;
};

const PROGRESS_EVERY_MS = 10_000;
const NEXT_COUNTDOWN_S = 10;

function readVolume() {
  try {
    const v = Number(localStorage.getItem("bv.volume"));
    return Number.isFinite(v) && v > 0 ? Math.min(1, v) : 1;
  } catch {
    return 1;
  }
}

export function PlayerPage() {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const router = useRouter();
  const qc = useQueryClient();
  const params = useParams({ strict: false }) as { kind: Kind; id: string };
  const { restart } = useSearch({ strict: false }) as { restart?: boolean };
  const kind: Kind = params.kind === "episode" ? "episode" : "movie";
  const id = Number(params.id);
  // Desktop app: a downloaded copy is played from disk, and is the only option offline.
  const offlineRoute = useRouterState({
    select: (s) => s.location.pathname.startsWith("/offline"),
  });
  const localQuery = useQuery({ ...localPlaybackQuery(kind, id), enabled: !!desktop });
  const local = localQuery.data ?? null;
  const localPending = !!desktop && localQuery.isPending;
  const localRef = useRef(local);
  localRef.current = local;

  const movie = useQuery({ ...movieQuery(id), enabled: kind === "movie" && !offlineRoute });
  const episode = useQuery({ ...episodeQuery(id), enabled: kind === "episode" && !offlineRoute });
  const title =
    (kind === "movie" ? movie.data?.title : episode.data?.series_title) ?? local?.meta.title;
  const ep = episode.data
    ? {
        season: episode.data.season_number,
        number: episode.data.episode_number,
        title: episode.data.title,
      }
    : local && kind === "episode"
      ? {
          season: local.meta.seasonNumber,
          number: local.meta.episodeNumber,
          title: local.meta.episodeTitle,
        }
      : null;
  const subtitle = ep
    ? `${episodeCode(ep.season, ep.number)}${ep.title ? ` — ${ep.title}` : ""}`
    : null;

  const containerRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const hlsRef = useRef<Hls | null>(null);
  const loadedUrl = useRef<string | null>(null);
  const capabilities = useMemo(() => detectCapabilities(), []);

  const [info, setInfo] = useState<Info | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [waiting, setWaiting] = useState(true);
  const [time, setTime] = useState(0); // media time, ms
  const [bufferedEnd, setBufferedEnd] = useState(0); // media time, ms
  const [volume, setVolume] = useState(readVolume);
  const [muted, setMuted] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [controlsVisible, setControlsVisible] = useState(true);
  const [panel, setPanel] = useState(false);
  const [countdown, setCountdown] = useState<number | null>(null);
  const [cue, setCue] = useState<string | null>(null);

  const offset = info?.hls ? info.start_ms : 0;
  const duration = info?.duration_ms ?? 0;
  const timeRef = useRef(0);
  timeRef.current = time;
  const infoRef = useRef<Info | null>(null);
  infoRef.current = info;

  // ---- Session ---------------------------------------------------------------------------
  const start = useCallback(
    async (opts: StartOptions = {}) => {
      if (localPending) return;
      setWaiting(true);
      setError(null);
      if (local) {
        setInfo(localInfo(local, opts, restart));
        return;
      }
      if (offlineRoute) {
        setError(t("player.error"));
        setWaiting(false);
        return;
      }
      try {
        const next = await call(
          api.POST("/api/playback", {
            body: {
              kind,
              id,
              capabilities,
              start_ms:
                opts.startMs !== undefined ? Math.round(opts.startMs) : restart ? 0 : undefined,
              audio_index: opts.audio ?? undefined,
              subtitle: opts.subtitle === undefined ? undefined : (opts.subtitle ?? "off"),
              remember: opts.remember ?? false,
            },
          }),
        );
        const previous = infoRef.current;
        if (previous?.session_id && previous.session_id !== next.session_id) {
          void api.POST("/api/playback/stop", { body: { session_id: previous.session_id } });
        }
        setInfo(next);
      } catch (e) {
        setError(e instanceof ApiError ? e.message : t("player.error"));
        setWaiting(false);
      }
    },
    [kind, id, capabilities, restart, t, local, localPending, offlineRoute],
  );

  useEffect(() => {
    void start();
  }, [start]);

  // Load the stream into the <video>.
  useEffect(() => {
    const video = videoRef.current;
    if (!video || !info) return;
    if (loadedUrl.current === info.url) return; // same file: only the subtitles changed
    loadedUrl.current = info.url;
    hlsRef.current?.destroy();
    hlsRef.current = null;

    const seekAfterLoad = info.hls ? 0 : info.start_ms / 1000;
    const onLoaded = () => {
      if (seekAfterLoad > 0) video.currentTime = seekAfterLoad;
      void video.play().catch(() => setPlaying(false));
    };
    video.addEventListener("loadedmetadata", onLoaded, { once: true });

    if (info.hls && Hls.isSupported()) {
      const hls = new Hls({ startPosition: 0, maxBufferLength: 60, backBufferLength: 120 });
      hls.on(Hls.Events.ERROR, (_, data) => {
        if (data.fatal) {
          if (data.type === Hls.ErrorTypes.MEDIA_ERROR) hls.recoverMediaError();
          else setError(t("player.error"));
        }
      });
      hls.loadSource(info.url);
      hls.attachMedia(video);
      hlsRef.current = hls;
    } else {
      video.src = info.url;
    }
    return () => video.removeEventListener("loadedmetadata", onLoaded);
  }, [info, t]);

  // Subtitles are drawn by us (consistent look, above the controls): the track is
  // kept "hidden" and its active cues are mirrored into state.
  useEffect(() => {
    const video = videoRef.current;
    setCue(null);
    if (!video || !info?.subtitle_url) return;
    const attach = () => {
      const track = video.textTracks[0];
      if (!track) return undefined;
      track.mode = "hidden";
      const onCue = () => {
        const cues = track.activeCues ? [...track.activeCues] : [];
        const text = cues
          .map((c) => (c as VTTCue).text.replace(/<[^>]+>/g, ""))
          .join("\n")
          .trim();
        setCue(text || null);
      };
      track.addEventListener("cuechange", onCue);
      return () => track.removeEventListener("cuechange", onCue);
    };
    let detach = attach();
    const onAdd = () => {
      detach?.();
      detach = attach();
    };
    video.textTracks.addEventListener("addtrack", onAdd);
    return () => {
      detach?.();
      video.textTracks.removeEventListener("addtrack", onAdd);
    };
  }, [info?.subtitle_url]);

  // Clean up on exit: stop the transcode and save the position.
  useEffect(() => {
    return () => {
      hlsRef.current?.destroy();
      const last = infoRef.current;
      const position = Math.round(timeRef.current);
      if (localRef.current) {
        if (position > 0)
          void desktop?.reportProgress({
            kind,
            id,
            positionMs: position,
            played: isPlayed(position, last?.duration_ms),
          });
        invalidateLibrary(qc);
        return;
      }
      const body = JSON.stringify({ kind, id, position_ms: position });
      void fetch("/api/progress", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body,
        keepalive: true,
      });
      if (last?.session_id) {
        void fetch("/api/playback/stop", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ session_id: last.session_id }),
          keepalive: true,
        });
      }
      invalidateLibrary(qc);
    };
  }, [kind, id, qc]);

  // ---- Progress --------------------------------------------------------------------------
  const report = useCallback(() => {
    if (timeRef.current <= 0) return;
    if (localRef.current) {
      const position = Math.round(timeRef.current);
      void desktop?.reportProgress({
        kind,
        id,
        positionMs: position,
        played: isPlayed(position, infoRef.current?.duration_ms),
      });
      return;
    }
    void api.POST("/api/progress", {
      body: { kind, id, position_ms: Math.round(timeRef.current) },
    });
  }, [kind, id]);

  useEffect(() => {
    if (!playing) return;
    const timer = setInterval(report, PROGRESS_EVERY_MS);
    return () => clearInterval(timer);
  }, [playing, report]);

  // ---- Video events ----------------------------------------------------------------------
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    video.volume = volume;
    video.muted = muted;
  }, [volume, muted]);

  const onTimeUpdate = () => {
    const video = videoRef.current;
    if (!video) return;
    setTime(offset + video.currentTime * 1000);
    const b = video.buffered;
    if (b.length) setBufferedEnd(offset + b.end(b.length - 1) * 1000);
  };

  const goNext = useCallback(() => {
    if (info?.next_episode_id)
      void navigate({
        to: offlineRoute ? "/offline/watch/$kind/$id" : "/watch/$kind/$id",
        params: { kind: "episode", id: String(info.next_episode_id) },
        replace: true,
      });
  }, [info, navigate, offlineRoute]);

  const onEnded = () => {
    setPlaying(false);
    report();
    if (info?.next_episode_id) setCountdown(NEXT_COUNTDOWN_S);
  };

  useEffect(() => {
    if (countdown === null) return;
    if (countdown <= 0) {
      setCountdown(null);
      goNext();
      return;
    }
    const timer = setTimeout(() => setCountdown((c) => (c === null ? null : c - 1)), 1000);
    return () => clearTimeout(timer);
  }, [countdown, goNext]);

  // ---- Commands --------------------------------------------------------------------------
  const togglePlay = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    if (video.paused) void video.play();
    else video.pause();
  }, []);

  const seekTo = useCallback(
    (targetMs: number) => {
      const video = videoRef.current;
      const current = infoRef.current;
      if (!video || !current) return;
      const target = Math.max(0, Math.min(targetMs, (current.duration_ms ?? targetMs) - 500));
      setTime(target);
      if (!current.hls) {
        video.currentTime = target / 1000;
        return;
      }
      // HLS: seek inside what's produced, otherwise restart the transcode there.
      const relative = (target - current.start_ms) / 1000;
      const produced = video.seekable.length ? video.seekable.end(video.seekable.length - 1) : 0;
      if (relative >= 0 && relative <= produced) video.currentTime = relative;
      else
        void start({
          startMs: target,
          audio: current.audio_index,
          subtitle: current.subtitle ?? null,
        });
    },
    [start],
  );

  const toggleFullscreen = useCallback(() => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void containerRef.current?.requestFullscreen();
  }, []);

  useEffect(() => {
    const onChange = () => setFullscreen(!!document.fullscreenElement);
    document.addEventListener("fullscreenchange", onChange);
    return () => document.removeEventListener("fullscreenchange", onChange);
  }, []);

  const back = useCallback(() => {
    if (document.fullscreenElement) void document.exitFullscreen();
    router.history.back();
  }, [router]);

  const changeTracks = (
    audio: number | null | undefined,
    subtitleId: string | null | undefined,
  ) => {
    setPanel(false);
    void start({ startMs: timeRef.current, audio, subtitle: subtitleId ?? null, remember: true });
  };

  // Keyboard shortcuts.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;
      const handled = true;
      switch (e.key) {
        case " ":
        case "k":
          togglePlay();
          break;
        case "ArrowLeft":
          seekTo(timeRef.current - 10_000);
          break;
        case "ArrowRight":
          seekTo(timeRef.current + 10_000);
          break;
        case "ArrowUp":
          setVolume((v) => Math.min(1, v + 0.1));
          break;
        case "ArrowDown":
          setVolume((v) => Math.max(0, v - 0.1));
          break;
        case "f":
          toggleFullscreen();
          break;
        case "m":
          setMuted((m) => !m);
          break;
        case "Escape":
          if (panel) setPanel(false);
          else if (!document.fullscreenElement) back();
          break;
        default:
          return;
      }
      if (handled) e.preventDefault();
      poke();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // Auto-hide controls while playing.
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const poke = useCallback(() => {
    setControlsVisible(true);
    if (hideTimer.current) clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => setControlsVisible(false), 3000);
  }, []);
  const showControls = controlsVisible || !playing || panel || !!error;

  useEffect(() => {
    try {
      localStorage.setItem("bv.volume", String(volume));
    } catch {
      // ignore
    }
  }, [volume]);

  const remainingNext = info?.next_episode_id && duration > 0 && duration - time < 120_000;

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: pointer movement only reveals the controls
    <div
      ref={containerRef}
      className={cx(
        "fixed inset-0 z-50 select-none bg-black",
        showControls ? "cursor-default" : "cursor-none",
      )}
      onMouseMove={poke}
      onTouchStart={poke}
    >
      {/* biome-ignore lint/a11y/useMediaCaption: subtitles come from the server as a <track> when chosen */}
      <video
        ref={videoRef}
        className="size-full"
        playsInline
        crossOrigin="anonymous"
        onClick={togglePlay}
        onDoubleClick={toggleFullscreen}
        onPlay={() => {
          setPlaying(true);
          poke();
        }}
        onPause={() => {
          setPlaying(false);
          report();
        }}
        onWaiting={() => setWaiting(true)}
        onPlaying={() => setWaiting(false)}
        onCanPlay={() => setWaiting(false)}
        onTimeUpdate={onTimeUpdate}
        onEnded={onEnded}
      >
        {info?.subtitle_url ? (
          <track key={info.subtitle_url} kind="subtitles" src={info.subtitle_url} default />
        ) : null}
      </video>

      <AnimatePresence>
        {cue ? (
          <motion.div
            key={cue}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className={cx(
              "pointer-events-none absolute inset-x-0 flex justify-center px-6 transition-[bottom] duration-300",
              showControls ? "bottom-36" : "bottom-[7%]",
            )}
          >
            <p className="max-w-4xl whitespace-pre-line rounded-lg bg-black/55 px-4 py-1.5 text-center text-[clamp(1rem,2.4vw,2rem)] font-medium leading-snug text-white [text-shadow:0_2px_6px_rgba(0,0,0,0.9)]">
              {cue}
            </p>
          </motion.div>
        ) : null}
      </AnimatePresence>

      <AnimatePresence>
        {waiting && !error ? (
          <motion.div
            className="pointer-events-none absolute inset-0 flex items-center justify-center"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
          >
            <Spinner className="size-14 border-[3px] text-accent" />
          </motion.div>
        ) : null}
      </AnimatePresence>

      {error ? (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-4 text-center">
          <p className="max-w-md text-lg">{error}</p>
          <button
            type="button"
            onClick={() => start({ startMs: time })}
            className="cursor-pointer rounded-full bg-accent px-5 py-2 font-semibold text-accent-ink"
          >
            {t("player.retry")}
          </button>
        </div>
      ) : null}

      <AnimatePresence>
        {showControls ? (
          <motion.div
            className="pointer-events-none absolute inset-0 flex flex-col justify-between"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.25 }}
          >
            <div className="pointer-events-auto flex items-center gap-3 bg-gradient-to-b from-black/80 to-transparent px-4 pb-16 pt-4 sm:px-8">
              <IconButton label={t("common.back")} onClick={back}>
                <ArrowLeft className="size-6" />
              </IconButton>
              <div className="min-w-0">
                <p className="truncate text-lg font-semibold">{title}</p>
                {subtitle ? <p className="truncate text-sm text-muted">{subtitle}</p> : null}
              </div>
            </div>

            <div className="pointer-events-auto bg-gradient-to-t from-black/90 via-black/50 to-transparent px-4 pb-5 pt-20 sm:px-8">
              <SeekBar
                time={time}
                duration={duration}
                buffered={bufferedEnd}
                offset={offset}
                onSeek={seekTo}
              />
              <div className="mt-3 flex items-center gap-1 sm:gap-2">
                <IconButton
                  label={playing ? t("player.pause") : t("player.play")}
                  onClick={togglePlay}
                  className="size-12"
                >
                  {playing ? (
                    <Pause className="size-7" fill="currentColor" />
                  ) : (
                    <Play className="size-7" fill="currentColor" />
                  )}
                </IconButton>
                <IconButton label={t("player.back10")} onClick={() => seekTo(time - 10_000)}>
                  <RotateCcw className="size-5" />
                </IconButton>
                <IconButton label={t("player.forward10")} onClick={() => seekTo(time + 10_000)}>
                  <RotateCw className="size-5" />
                </IconButton>
                <VolumeControl
                  volume={muted ? 0 : volume}
                  onChange={(v) => {
                    setMuted(false);
                    setVolume(v);
                  }}
                  onToggleMute={() => setMuted((m) => !m)}
                />
                <span className="ml-2 font-mono text-sm tabular-nums text-fg/80">
                  {formatClock(time / 1000)}{" "}
                  <span className="text-faint">/ {formatClock(duration / 1000)}</span>
                </span>
                <div className="ml-auto flex items-center gap-1 sm:gap-2">
                  {info?.next_episode_id ? (
                    <motion.button
                      type="button"
                      onClick={goNext}
                      animate={remainingNext ? { scale: [1, 1.06, 1] } : {}}
                      transition={{
                        repeat: remainingNext ? Number.POSITIVE_INFINITY : 0,
                        duration: 1.6,
                      }}
                      className={cx(
                        "flex cursor-pointer items-center gap-2 rounded-full px-4 py-2 text-sm font-semibold transition",
                        remainingNext ? "bg-accent text-accent-ink" : "hover:bg-white/10",
                      )}
                    >
                      <SkipForward className="size-4" fill="currentColor" />
                      <span className="hidden sm:inline">{t("player.nextEpisode")}</span>
                    </motion.button>
                  ) : null}
                  <IconButton
                    label={t("player.tracks")}
                    onClick={() => setPanel((p) => !p)}
                    className={panel ? "bg-white/15" : ""}
                  >
                    <Subtitles className="size-5" />
                  </IconButton>
                  <IconButton label={t("player.fullscreen")} onClick={toggleFullscreen}>
                    {fullscreen ? <Minimize className="size-5" /> : <Maximize className="size-5" />}
                  </IconButton>
                </div>
              </div>
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>

      <AnimatePresence>
        {panel && info ? (
          <TracksPanel
            info={info}
            language={i18n.language}
            onClose={() => setPanel(false)}
            onChange={changeTracks}
          />
        ) : null}
      </AnimatePresence>

      <AnimatePresence>
        {countdown !== null ? (
          <NextUpOverlay
            seconds={countdown}
            onPlay={() => {
              setCountdown(null);
              goNext();
            }}
            onCancel={() => setCountdown(null)}
          />
        ) : null}
      </AnimatePresence>
    </div>
  );
}

function SeekBar({
  time,
  duration,
  buffered,
  offset,
  onSeek,
}: {
  time: number;
  duration: number;
  buffered: number;
  offset: number;
  onSeek: (ms: number) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<number | null>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const ratioAt = (clientX: number) => {
    const rect = ref.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) return 0;
    return Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
  };
  const pct = (ms: number) =>
    duration > 0 ? `${(Math.min(ms, duration) / duration) * 100}%` : "0%";
  const shown = drag ?? time;
  return (
    <div
      ref={ref}
      role="slider"
      tabIndex={-1}
      aria-valuemin={0}
      aria-valuemax={duration}
      aria-valuenow={Math.round(shown)}
      className="group relative flex h-5 cursor-pointer items-center"
      onPointerMove={(e) => {
        const r = ratioAt(e.clientX);
        setHover(r * duration);
        if (drag !== null) setDrag(r * duration);
      }}
      onPointerLeave={() => setHover(null)}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        setDrag(ratioAt(e.clientX) * duration);
      }}
      onPointerUp={(e) => {
        const target = ratioAt(e.clientX) * duration;
        setDrag(null);
        onSeek(target);
      }}
    >
      <div className="relative h-1 w-full overflow-hidden rounded-full bg-white/20 transition-all group-hover:h-1.5">
        {offset > 0 ? (
          <div className="absolute inset-y-0 left-0 bg-white/5" style={{ width: pct(offset) }} />
        ) : null}
        <div
          className="absolute inset-y-0 bg-white/35"
          style={{ left: pct(offset), width: `calc(${pct(buffered)} - ${pct(offset)})` }}
        />
        <div className="absolute inset-y-0 left-0 bg-accent" style={{ width: pct(shown) }} />
      </div>
      <div
        className="absolute size-3.5 -translate-x-1/2 scale-0 rounded-full bg-accent shadow-lg transition-transform group-hover:scale-100"
        style={{ left: pct(shown) }}
      />
      {hover !== null ? (
        <div
          className="absolute -top-9 -translate-x-1/2 rounded-md bg-black/85 px-2 py-1 font-mono text-xs tabular-nums"
          style={{ left: pct(hover) }}
        >
          {formatClock(hover / 1000)}
        </div>
      ) : null}
    </div>
  );
}

function VolumeControl({
  volume,
  onChange,
  onToggleMute,
}: {
  volume: number;
  onChange: (v: number) => void;
  onToggleMute: () => void;
}) {
  const { t } = useTranslation();
  const Icon = volume === 0 ? VolumeX : volume < 0.5 ? Volume1 : Volume2;
  return (
    <div className="group/vol flex items-center">
      <IconButton label={t("player.mute")} onClick={onToggleMute}>
        <Icon className="size-5" />
      </IconButton>
      <input
        type="range"
        min={0}
        max={1}
        step={0.05}
        value={volume}
        aria-label={t("player.volume")}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-0 cursor-pointer accent-[var(--color-accent)] opacity-0 transition-all duration-300 group-hover/vol:w-24 group-hover/vol:opacity-100 focus:w-24 focus:opacity-100"
      />
    </div>
  );
}

function kindLabel(t: TFunction, kind: Schemas["TrackKind"]) {
  return kind === "normal" ? null : t(`player.kind.${kind}`);
}

function TracksPanel({
  info,
  language,
  onChange,
  onClose,
}: {
  info: Info;
  language: string;
  onChange: (audio: number | null | undefined, subtitle: string | null) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  return (
    <>
      <button
        type="button"
        aria-label={t("common.close")}
        className="absolute inset-0 cursor-default"
        onClick={onClose}
      />
      <motion.div
        initial={{ opacity: 0, y: 16, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 12, scale: 0.98 }}
        transition={{ type: "spring", duration: 0.35, bounce: 0.15 }}
        className="absolute bottom-28 right-4 grid max-h-[60vh] w-[min(36rem,calc(100vw-2rem))] grid-cols-2 gap-6 overflow-y-auto rounded-2xl border border-white/10 bg-black/85 p-5 shadow-2xl backdrop-blur-xl sm:right-8"
      >
        <TrackColumn title={t("player.audio")}>
          {info.audio_tracks.map((a) => (
            <TrackOption
              key={a.index}
              selected={info.audio_index === a.index}
              onClick={() => onChange(a.index, info.subtitle ?? null)}
              label={languageName(a.language, language) ?? t("player.unknownLanguage")}
              detail={[
                kindLabel(t, a.kind),
                a.title,
                a.channels && a.channels > 2
                  ? `${a.channels === 6 ? "5.1" : a.channels === 8 ? "7.1" : `${a.channels}ch`}`
                  : null,
              ]
                .filter(Boolean)
                .join(" · ")}
            />
          ))}
        </TrackColumn>
        <TrackColumn title={t("player.subtitles")}>
          <TrackOption
            selected={!info.subtitle}
            onClick={() => onChange(info.audio_index, null)}
            label={t("player.off")}
          />
          {info.subtitles.map((s) => (
            <TrackOption
              key={s.id}
              selected={info.subtitle === s.id}
              onClick={() => onChange(info.audio_index, s.id)}
              label={languageName(s.language, language) ?? t("player.unknownLanguage")}
              detail={[
                kindLabel(t, s.kind),
                s.external ? t("player.external") : null,
                s.text ? null : t("player.burned"),
              ]
                .filter(Boolean)
                .join(" · ")}
            />
          ))}
        </TrackColumn>
      </motion.div>
    </>
  );
}

function TrackColumn({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div>
      <p className="mb-2 text-xs font-semibold uppercase tracking-wider text-faint">{title}</p>
      <div className="flex flex-col">{children}</div>
    </div>
  );
}

function TrackOption({
  selected,
  label,
  detail,
  onClick,
}: {
  selected: boolean;
  label: string;
  detail?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cx(
        "flex cursor-pointer items-start gap-2 rounded-lg px-2 py-2 text-left text-sm transition-colors hover:bg-white/10",
        selected ? "text-fg" : "text-muted",
      )}
    >
      <Check
        className={cx("mt-0.5 size-4 shrink-0 text-accent", selected ? "opacity-100" : "opacity-0")}
      />
      <span>
        <span className={selected ? "font-semibold" : ""}>{label}</span>
        {detail ? <span className="block text-xs text-faint">{detail}</span> : null}
      </span>
    </button>
  );
}

function NextUpOverlay({
  seconds,
  onPlay,
  onCancel,
}: {
  seconds: number;
  onPlay: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const radius = 22;
  const circumference = 2 * Math.PI * radius;
  return (
    <motion.div
      initial={{ opacity: 0, x: 40 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, x: 40 }}
      className="absolute bottom-32 right-4 flex items-center gap-4 rounded-2xl border border-white/10 bg-black/85 p-4 shadow-2xl backdrop-blur-xl sm:right-8"
    >
      <button
        type="button"
        onClick={onPlay}
        className="relative flex size-14 cursor-pointer items-center justify-center"
        aria-label={t("player.nextEpisode")}
      >
        <svg viewBox="0 0 52 52" aria-hidden="true" className="absolute inset-0 -rotate-90">
          <circle
            cx="26"
            cy="26"
            r={radius}
            fill="none"
            stroke="rgb(255 255 255 / 0.15)"
            strokeWidth="3"
          />
          <motion.circle
            cx="26"
            cy="26"
            r={radius}
            fill="none"
            stroke="var(--color-accent)"
            strokeWidth="3"
            strokeLinecap="round"
            strokeDasharray={circumference}
            animate={{ strokeDashoffset: circumference * (seconds / NEXT_COUNTDOWN_S) }}
            transition={{ duration: 1, ease: "linear" }}
          />
        </svg>
        <SkipForward className="size-5" fill="currentColor" />
      </button>
      <div>
        <p className="font-semibold">{t("player.nextEpisode")}</p>
        <p className="text-sm text-muted">{t("player.nextIn", { seconds })}</p>
      </div>
      <button
        type="button"
        onClick={onCancel}
        className="ml-2 cursor-pointer rounded-full px-3 py-1.5 text-sm text-muted transition hover:bg-white/10 hover:text-fg"
      >
        {t("common.cancel")}
      </button>
    </motion.div>
  );
}
