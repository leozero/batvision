import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import type { TFunction } from "i18next";
import { Check, ChevronDown, Eye, EyeOff, MoreHorizontal, Play } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { DetailHeader, Dot, IdentifyButton } from "../components/detail";
import { DownloadSeriesButton, SeriesWatchlistButton } from "../components/downloads";
import { Img } from "../components/media";
import { pickRating, Rating } from "../components/rating";
import { DownloadedMark } from "../components/ThisComputer";
import {
  Button,
  buttonClass,
  IconButton,
  LoadError,
  MenuContent,
  MenuItem,
  MenuRoot,
  MenuTrigger,
  Page,
  ProgressBar,
  Skeleton,
} from "../components/ui";
import { ApiError, imageUrl, type Schemas } from "../lib/api";
import { episodeCode, formatRuntime, progressRatio } from "../lib/format";
import { episodeQuery, seasonQuery, seriesQuery, useSetPlayed } from "../lib/queries";
import { NotFoundPage } from "./NotFoundPage";

function seasonLabel(t: TFunction, n: number) {
  return n === 0 ? t("detail.specials") : t("detail.season", { number: n });
}

function EpisodeRow({ episode, index }: { episode: Schemas["EpisodeSummary"]; index: number }) {
  const { t, i18n } = useTranslation();
  const setPlayed = useSetPlayed();
  const duration =
    episode.duration_ms ?? (episode.runtime_minutes ? episode.runtime_minutes * 60_000 : null);
  const progress = progressRatio(episode.position_ms, duration);
  const number = episode.episode_number_end
    ? `${episode.episode_number}-${episode.episode_number_end}`
    : String(episode.episode_number);
  return (
    <motion.li
      layout
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.35, delay: Math.min(index, 15) * 0.035 }}
      className="group relative"
    >
      <Link
        to="/watch/$kind/$id"
        params={{ kind: "episode", id: String(episode.id) }}
        className="flex flex-col gap-4 rounded-2xl p-3 transition-colors hover:bg-white/[0.04] sm:flex-row sm:items-center sm:gap-6"
      >
        <span className="hidden w-8 shrink-0 text-center text-2xl font-light text-faint sm:block">
          {number}
        </span>
        <div className="relative aspect-video w-full shrink-0 overflow-hidden rounded-xl bg-surface-2 ring-1 ring-white/5 sm:w-56">
          <Img
            src={imageUrl(episode.still_path, "w500")}
            alt=""
            className="size-full object-cover transition-transform duration-500 group-hover:scale-105"
          />
          <span className="absolute inset-0 flex items-center justify-center bg-black/30 opacity-0 transition-opacity group-hover:opacity-100">
            <span className="flex size-11 items-center justify-center rounded-full bg-accent text-accent-ink shadow-lg">
              <Play className="ml-0.5 size-5" fill="currentColor" />
            </span>
          </span>
          {episode.played ? (
            <span className="absolute right-2 top-2 flex size-6 items-center justify-center rounded-full bg-accent text-accent-ink">
              <Check className="size-3.5" strokeWidth={3} />
            </span>
          ) : null}
          {progress > 0 && !episode.played ? (
            <ProgressBar value={progress} className="absolute inset-x-2 bottom-2" />
          ) : null}
        </div>
        <div className="min-w-0 flex-1 pr-10">
          <div className="flex items-baseline justify-between gap-3">
            <p className="truncate font-semibold">
              <span className="text-faint sm:hidden">{number}. </span>
              {episode.title ?? t("detail.episode", { number })}
            </p>
            <span className="flex shrink-0 items-center gap-3 text-sm text-faint">
              <DownloadedMark kind="episode" id={episode.id} />
              <Rating ratings={episode.ratings} />
              {duration ? formatRuntime(duration, i18n.language) : null}
            </span>
          </div>
          {episode.overview ? (
            <p className="mt-1.5 line-clamp-2 text-sm leading-relaxed text-muted">
              {episode.overview}
            </p>
          ) : null}
          {episode.air_date ? (
            <p className="mt-1.5 text-xs text-faint">
              {new Date(episode.air_date).toLocaleDateString(i18n.language, {
                dateStyle: "medium",
              })}
            </p>
          ) : null}
        </div>
      </Link>
      <div className="absolute right-3 top-3 sm:top-1/2 sm:-translate-y-1/2">
        <MenuRoot>
          <MenuTrigger asChild>
            <IconButton label={t("common.more")} className="opacity-60 group-hover:opacity-100">
              <MoreHorizontal className="size-5" />
            </IconButton>
          </MenuTrigger>
          <MenuContent>
            <MenuItem
              onSelect={() =>
                setPlayed.mutate({ kind: "episode", id: episode.id, played: !episode.played })
              }
            >
              {episode.played ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
              {episode.played ? t("detail.markUnwatched") : t("detail.markWatched")}
            </MenuItem>
          </MenuContent>
        </MenuRoot>
      </div>
    </motion.li>
  );
}

export function SeriesDetailPage() {
  const { t } = useTranslation();
  const { id } = useParams({ strict: false }) as { id: string };
  const search = useSearch({ strict: false }) as { season?: number };
  const navigate = useNavigate();
  const seriesId = Number(id);
  const { data: series, isPending, error, refetch } = useQuery(seriesQuery(seriesId));
  const setPlayed = useSetPlayed();

  const seasons = series?.seasons ?? [];
  const defaultSeason = series?.up_next?.season_number ?? seasons[0]?.season_number ?? 1;
  const season = (n: number) => seasons.find((s) => s.season_number === n);
  // A `?season=` that doesn't exist (old link, season removed) falls back to the default one.
  const current = season(search.season ?? defaultSeason) ?? season(defaultSeason) ?? seasons[0];
  const selected = current?.season_number ?? defaultSeason;
  const episodesQuery = useQuery({
    ...seasonQuery(seriesId, selected),
    enabled: !!series && !!current,
  });
  const episodes = episodesQuery.data;
  // A disabled query stays pending: without a season there is nothing to wait for.
  const episodesPending = !!current && episodesQuery.isPending;

  // Audio languages offered when downloading: those of the next (or first) episode.
  const sampleEpisode = series?.up_next?.id ?? episodes?.[0]?.id;
  const { data: sample } = useQuery({
    ...episodeQuery(sampleEpisode ?? 0),
    enabled: sampleEpisode !== undefined,
  });

  if (isPending) {
    return (
      <Page className="mx-auto max-w-6xl px-4 pt-28 sm:px-10">
        <Skeleton className="mb-4 h-12 w-2/3" />
        <Skeleton className="h-24 w-full" />
      </Page>
    );
  }
  // Only a 404 means the series is gone: anything else can be retried.
  if (error instanceof ApiError && error.status === 404) return <NotFoundPage />;
  if (!series) {
    return (
      <Page className="px-4 pt-16">
        <LoadError onRetry={refetch} />
      </Page>
    );
  }

  const upNext = series.up_next;
  const allPlayed = seasons.length > 0 && !upNext;
  const episodeCount = seasons.reduce((n, s) => n + s.episode_count, 0);

  return (
    <Page>
      <DetailHeader
        backdrop={series.backdrop_path}
        poster={series.poster_path}
        title={series.title}
        overview={series.overview}
        meta={
          <>
            {series.year ? <span>{series.year}</span> : null}
            <Dot />
            <span>
              {t("detail.seasonCount", {
                count: seasons.filter((s) => s.season_number > 0).length,
              })}
            </span>
            <Dot />
            <span>{t("detail.episodeCount", { count: episodeCount })}</span>
            {pickRating(series.ratings) ? (
              <>
                <Dot />
                <Rating ratings={series.ratings} className="font-semibold" />
              </>
            ) : null}
          </>
        }
        actions={
          <>
            {upNext ? (
              <Link
                to="/watch/$kind/$id"
                params={{ kind: "episode", id: String(upNext.id) }}
                className={buttonClass({ variant: "primary", size: "lg" })}
              >
                <Play className="size-5" fill="currentColor" />
                {upNext.position_ms > 0 ? t("common.resume") : t("common.play")}{" "}
                <span className="font-medium opacity-80">
                  {episodeCode(upNext.season_number, upNext.episode_number)}
                </span>
              </Link>
            ) : null}
            <Button
              size="lg"
              variant="ghost"
              loading={setPlayed.isPending}
              onClick={() => setPlayed.mutate({ kind: "series", id: seriesId, played: !allPlayed })}
            >
              {allPlayed ? <EyeOff className="size-5" /> : <Eye className="size-5" />}
              {allPlayed ? t("detail.markUnwatched") : t("detail.markSeriesWatched")}
            </Button>
            <SeriesWatchlistButton series={series} />
            <DownloadSeriesButton series={series} episodeFile={sample?.file} />
            <IdentifyButton kind="series" id={seriesId} title={series.title} year={series.year} />
          </>
        }
      />

      <section className="mx-auto max-w-6xl px-4 sm:px-10">
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <MenuRoot>
              <MenuTrigger asChild>
                <button
                  type="button"
                  className="flex cursor-pointer items-center gap-2 rounded-full bg-white/8 px-5 py-2.5 text-lg font-semibold transition hover:bg-white/12"
                >
                  {seasonLabel(t, selected)}
                  <ChevronDown className="size-5 text-muted" />
                </button>
              </MenuTrigger>
              <MenuContent align="start">
                {seasons.map((s) => (
                  <MenuItem
                    key={s.id}
                    onSelect={() =>
                      navigate({
                        to: "/series/$id",
                        params: { id },
                        search: { season: s.season_number },
                        replace: true,
                      })
                    }
                  >
                    <span className="flex-1">{seasonLabel(t, s.season_number)}</span>
                    <Rating ratings={s.ratings} className="text-xs" />
                    <span className="text-xs text-faint">
                      {t("detail.episodeCount", { count: s.episode_count })}
                    </span>
                    {s.season_number === selected ? <Check className="size-4 text-accent" /> : null}
                  </MenuItem>
                ))}
              </MenuContent>
            </MenuRoot>
            <Rating ratings={current?.ratings} className="text-sm font-semibold" />
          </div>
          {current ? (
            <Button
              variant="ghost"
              size="sm"
              // Until the episodes are known, "watched" or "unwatched" would be a guess.
              disabled={!episodes}
              onClick={() =>
                setPlayed.mutate({
                  kind: "season",
                  id: current.id,
                  played: !(episodes ?? []).every((e) => e.played),
                })
              }
            >
              <Check className="size-4" />
              {(episodes ?? []).every((e) => e.played)
                ? t("detail.markSeasonUnwatched")
                : t("detail.markSeasonWatched")}
            </Button>
          ) : null}
        </div>

        <AnimatePresence mode="wait">
          <motion.ul
            key={selected}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="divide-y divide-line/60"
          >
            {episodesPending
              ? Array.from({ length: 4 }, (_, i) => (
                  <li key={i} className="flex gap-6 p-3">
                    <Skeleton className="aspect-video w-56" />
                    <div className="flex-1 space-y-2">
                      <Skeleton className="h-5 w-1/2" />
                      <Skeleton className="h-4 w-full" />
                    </div>
                  </li>
                ))
              : episodes?.map((episode, i) => (
                  <EpisodeRow key={episode.id} episode={episode} index={i} />
                ))}
          </motion.ul>
        </AnimatePresence>
        {current && episodesQuery.isError && !episodes ? (
          <LoadError onRetry={episodesQuery.refetch} />
        ) : null}
      </section>
    </Page>
  );
}
