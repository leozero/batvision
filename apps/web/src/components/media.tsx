import { Link } from "@tanstack/react-router";
import { Check, ChevronLeft, ChevronRight, Film, Info, Play, Tv } from "lucide-react";
import { motion } from "motion/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { imageUrl, type Schemas } from "../lib/api";
import { episodeCode, formatRuntime, progressRatio } from "../lib/format";
import { pickRating, Rating } from "./rating";
import { Button, cx, ProgressBar, Skeleton } from "./ui";

/** Image that fades in once loaded, with a placeholder when missing or broken. */
export function Img({
  src,
  alt,
  className,
  fallback,
}: {
  src?: string;
  alt: string;
  className?: string;
  fallback?: ReactNode;
}) {
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  if (!src || failed) {
    return (
      <div
        className={cx(
          "flex items-center justify-center bg-gradient-to-br from-surface-3 to-surface-2",
          className,
        )}
      >
        {fallback}
      </div>
    );
  }
  return (
    <img
      src={src}
      alt={alt}
      loading="lazy"
      decoding="async"
      onLoad={() => setLoaded(true)}
      onError={() => setFailed(true)}
      className={cx(
        "transition-opacity duration-500",
        loaded ? "opacity-100" : "opacity-0",
        className,
      )}
    />
  );
}

function PosterFallback({ title, kind }: { title: string; kind: "movie" | "series" }) {
  const Icon = kind === "movie" ? Film : Tv;
  return (
    <div className="flex flex-col items-center gap-2 px-3 text-center">
      <Icon className="size-7 text-faint" />
      <span className="line-clamp-3 text-sm font-medium text-muted">{title}</span>
    </div>
  );
}

export type PosterItem = {
  id: number;
  title: string;
  year?: number | null;
  poster_path?: string | null;
  played?: boolean;
  position_ms?: number;
  duration_ms?: number | null;
  unplayed_count?: number;
  ratings?: Schemas["Rating"][];
};

export function PosterCard({
  item,
  kind,
  index = 0,
}: {
  item: PosterItem;
  kind: "movie" | "series";
  index?: number;
}) {
  const progress = progressRatio(item.position_ms ?? 0, item.duration_ms);
  const to = kind === "movie" ? "/movies/$id" : "/series/$id";
  return (
    <motion.div
      initial={{ opacity: 0, y: 16 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.4, delay: Math.min(index, 12) * 0.03, ease: [0.22, 1, 0.36, 1] }}
    >
      <Link to={to} params={{ id: String(item.id) }} className="group block outline-none">
        <motion.div
          whileHover={{ y: -6, scale: 1.03 }}
          transition={{ type: "spring", stiffness: 320, damping: 22 }}
          className="relative aspect-[2/3] overflow-hidden rounded-[var(--radius-card)] bg-surface-2 shadow-lg ring-1 ring-white/5 transition-shadow duration-300 group-hover:shadow-2xl group-hover:shadow-black/60 group-hover:ring-accent/40 group-focus-visible:ring-2 group-focus-visible:ring-accent"
        >
          <Img
            src={imageUrl(item.poster_path, "w342")}
            alt={item.title}
            className="size-full object-cover"
            fallback={<PosterFallback title={item.title} kind={kind} />}
          />
          <div className="absolute inset-0 bg-gradient-to-t from-black/70 via-transparent to-transparent opacity-0 transition-opacity duration-300 group-hover:opacity-100" />
          {item.played || item.unplayed_count === 0 ? (
            <span className="absolute right-2 top-2 flex size-6 items-center justify-center rounded-full bg-accent text-accent-ink shadow">
              <Check className="size-3.5" strokeWidth={3} />
            </span>
          ) : item.unplayed_count ? (
            <span className="absolute right-2 top-2 rounded-full bg-black/70 px-2 py-0.5 text-xs font-semibold backdrop-blur">
              {item.unplayed_count}
            </span>
          ) : null}
          {progress > 0 ? (
            <ProgressBar value={progress} className="absolute inset-x-2 bottom-2" />
          ) : null}
        </motion.div>
        <div className="mt-2.5 px-0.5">
          <p className="truncate text-sm font-medium text-fg/90 transition-colors group-hover:text-fg">
            {item.title}
          </p>
          {item.year || pickRating(item.ratings) ? (
            <p className="flex items-center gap-1.5 text-xs text-faint">
              {item.year ? <span>{item.year}</span> : null}
              {item.year && pickRating(item.ratings) ? <span aria-hidden>·</span> : null}
              <Rating ratings={item.ratings} />
            </p>
          ) : null}
        </div>
      </Link>
    </motion.div>
  );
}

/** Landscape card for "Continue watching" / "Next up": starts playback directly. */
export function WideCard({ item, index = 0 }: { item: Schemas["HomeItem"]; index?: number }) {
  const { t } = useTranslation();
  const progress = progressRatio(item.position_ms, item.duration_ms);
  const isEpisode = item.kind === "episode";
  return (
    <motion.div
      initial={{ opacity: 0, x: 24 }}
      animate={{ opacity: 1, x: 0 }}
      transition={{ duration: 0.45, delay: Math.min(index, 8) * 0.05, ease: [0.22, 1, 0.36, 1] }}
      className="w-72 shrink-0 sm:w-80"
    >
      <Link
        to="/watch/$kind/$id"
        params={{ kind: item.kind, id: String(item.id) }}
        className="group block outline-none"
      >
        <motion.div
          whileHover={{ scale: 1.03 }}
          transition={{ type: "spring", stiffness: 300, damping: 22 }}
          className="relative aspect-video overflow-hidden rounded-[var(--radius-card)] bg-surface-2 ring-1 ring-white/5 transition-shadow group-hover:shadow-2xl group-hover:shadow-black/60 group-hover:ring-accent/40"
        >
          <Img
            src={imageUrl(item.image_path, "w780")}
            alt={item.title}
            className="size-full object-cover transition-transform duration-700 group-hover:scale-105"
            fallback={<PosterFallback title={item.title} kind={isEpisode ? "series" : "movie"} />}
          />
          <div className="absolute inset-0 bg-gradient-to-t from-black/85 via-black/10 to-transparent" />
          <span className="absolute left-1/2 top-1/2 flex size-12 -translate-x-1/2 -translate-y-1/2 scale-75 items-center justify-center rounded-full bg-accent text-accent-ink opacity-0 shadow-xl transition-all duration-300 group-hover:scale-100 group-hover:opacity-100">
            <Play className="ml-0.5 size-5" fill="currentColor" />
          </span>
          <div className="absolute inset-x-3 bottom-3">
            {isEpisode ? (
              <p className="text-xs font-semibold text-accent">
                {episodeCode(item.season_number, item.episode_number)}
              </p>
            ) : null}
            <p className="truncate font-semibold">{item.title}</p>
            {item.subtitle ? <p className="truncate text-xs text-muted">{item.subtitle}</p> : null}
            {progress > 0 ? <ProgressBar value={progress} className="mt-2" /> : null}
          </div>
        </motion.div>
      </Link>
      <span className="sr-only">{t("common.play")}</span>
    </motion.div>
  );
}

/** Horizontal row with scroll arrows (desktop) and swipe (touch). */
export function Row({
  title,
  action,
  children,
}: {
  title: string;
  /** A link next to the title ("See all"). */
  action?: ReactNode;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState({ start: true, end: false });
  const update = () => {
    const el = ref.current;
    if (!el) return;
    setEdges({
      start: el.scrollLeft < 8,
      end: el.scrollLeft + el.clientWidth >= el.scrollWidth - 8,
    });
  };
  useEffect(update, []);
  const scroll = (dir: number) =>
    ref.current?.scrollBy({ left: dir * ref.current.clientWidth * 0.85, behavior: "smooth" });
  return (
    <section className="group/row relative">
      <div className="mb-3 flex items-baseline gap-4 px-4 sm:px-10">
        <h2 className="text-lg font-semibold tracking-tight">{title}</h2>
        {action}
      </div>
      <div className="relative">
        <div
          ref={ref}
          onScroll={update}
          className="no-scrollbar flex gap-4 overflow-x-auto scroll-smooth px-4 pb-4 pt-2 sm:px-10"
        >
          {children}
        </div>
        {!edges.start ? (
          <button
            type="button"
            aria-label={t("common.previous")}
            onClick={() => scroll(-1)}
            className="absolute left-0 top-0 bottom-4 hidden w-12 cursor-pointer items-center justify-center bg-gradient-to-r from-bg to-transparent opacity-0 transition-opacity focus-visible:opacity-100 group-hover/row:opacity-100 sm:flex"
          >
            <ChevronLeft className="size-7" />
          </button>
        ) : null}
        {!edges.end ? (
          <button
            type="button"
            aria-label={t("common.next")}
            onClick={() => scroll(1)}
            className="absolute right-0 top-0 bottom-4 hidden w-12 cursor-pointer items-center justify-center bg-gradient-to-l from-bg to-transparent opacity-0 transition-opacity focus-visible:opacity-100 group-hover/row:opacity-100 sm:flex"
          >
            <ChevronRight className="size-7" />
          </button>
        ) : null}
      </div>
    </section>
  );
}

export function RowSkeleton({ wide }: { wide?: boolean }) {
  return (
    <div className="px-4 sm:px-10">
      <Skeleton className="mb-4 h-5 w-48" />
      <div className="flex gap-4 overflow-hidden">
        {Array.from({ length: 8 }, (_, i) => (
          <Skeleton
            key={i}
            className={wide ? "aspect-video w-80 shrink-0" : "aspect-[2/3] w-40 shrink-0"}
          />
        ))}
      </div>
    </div>
  );
}

/** Big banner at the top of the home page. */
export function Hero({ item }: { item: Schemas["HomeItem"] }) {
  const { t, i18n } = useTranslation();
  const resume = item.position_ms > 0;
  const detail =
    item.kind === "movie"
      ? ({ to: "/movies/$id", params: { id: String(item.id) } } as const)
      : ({ to: "/series/$id", params: { id: String(item.series_id) } } as const);
  return (
    <section className="relative isolate -mt-16 flex min-h-[78svh] items-end overflow-hidden">
      <motion.div
        className="absolute inset-0 -z-10"
        initial={{ scale: 1.12, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        transition={{ duration: 1.6, ease: [0.22, 1, 0.36, 1] }}
      >
        <Img
          src={imageUrl(item.backdrop_path ?? item.image_path, "original")}
          alt=""
          className="size-full object-cover"
        />
      </motion.div>
      <div className="absolute inset-0 -z-10 bg-gradient-to-t from-bg via-bg/40 to-bg/10" />
      <div className="absolute inset-0 -z-10 bg-gradient-to-r from-bg/90 via-bg/30 to-transparent" />
      <motion.div
        className="max-w-2xl px-4 pb-16 sm:px-10 sm:pb-20"
        initial="hidden"
        animate="show"
        variants={{ show: { transition: { staggerChildren: 0.08, delayChildren: 0.25 } } }}
      >
        {[
          item.kind === "episode" ? (
            <p
              key="code"
              className="mb-2 text-sm font-semibold uppercase tracking-widest text-accent"
            >
              {episodeCode(item.season_number, item.episode_number)}
              {item.subtitle ? ` — ${item.subtitle}` : ""}
            </p>
          ) : null,
          <h1 key="title" className="text-4xl font-black tracking-tight drop-shadow-lg sm:text-6xl">
            {item.title}
          </h1>,
          item.overview ? (
            <p
              key="overview"
              className="mt-4 line-clamp-3 max-w-xl text-base text-fg/80 sm:text-lg"
            >
              {item.overview}
            </p>
          ) : null,
          <div key="actions" className="mt-7 flex flex-wrap items-center gap-3">
            <Link to="/watch/$kind/$id" params={{ kind: item.kind, id: String(item.id) }}>
              <Button variant="primary" size="lg">
                <Play className="size-5" fill="currentColor" />
                {resume ? t("common.resume") : t("common.play")}
              </Button>
            </Link>
            <Link {...detail}>
              <Button size="lg">
                <Info className="size-5" />
                {t("common.moreInfo")}
              </Button>
            </Link>
            {resume && item.duration_ms ? (
              <span className="text-sm text-muted">
                {t("common.remaining", {
                  time: formatRuntime(item.duration_ms - item.position_ms, i18n.language),
                })}
              </span>
            ) : null}
          </div>,
        ]
          .filter(Boolean)
          .map((child, i) => (
            <motion.div
              key={i}
              variants={{
                hidden: { opacity: 0, y: 20 },
                show: { opacity: 1, y: 0, transition: { duration: 0.6 } },
              }}
            >
              {child}
            </motion.div>
          ))}
      </motion.div>
    </section>
  );
}

export function PosterGrid({ children }: { children: ReactNode }) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-x-4 gap-y-7 sm:grid-cols-[repeat(auto-fill,minmax(10.5rem,1fr))]">
      {children}
    </div>
  );
}

/** Calls `onVisible` when scrolled into view (infinite lists). */
export function LoadMore({ onVisible, active }: { onVisible: () => void; active: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el || !active) return;
    const observer = new IntersectionObserver(
      (entries) => entries[0]?.isIntersecting && onVisible(),
      {
        rootMargin: "800px",
      },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [onVisible, active]);
  return <div ref={ref} className="h-px" />;
}
