import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import { Check, Eye, EyeOff, Play, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { DetailHeader, Dot, IdentifyButton } from "../components/detail";
import { DownloadMovieButton, WatchlistButton } from "../components/downloads";
import { pickRating, Rating } from "../components/rating";
import { DownloadedMark } from "../components/ThisComputer";
import { Badge, Button, Page, ProgressBar, Skeleton } from "../components/ui";
import type { Schemas } from "../lib/api";
import { formatRuntime, languageName, progressRatio } from "../lib/format";
import { movieQuery, useSetPlayed } from "../lib/queries";
import { NotFoundPage } from "./NotFoundPage";

export function TechInfo({ file }: { file: Schemas["FileInfo"] | null | undefined }) {
  const { t, i18n } = useTranslation();
  const media = file?.media;
  if (!media) return null;
  const video = media.video;
  const resolution = video?.height
    ? video.height >= 2000
      ? "4K"
      : video.height >= 1000
        ? "1080p"
        : video.height >= 700
          ? "720p"
          : `${video.height}p`
    : null;
  const audio = [
    ...new Set(media.audio.map((a) => languageName(a.language, i18n.language)).filter(Boolean)),
  ];
  const subs = [
    ...new Set(
      [...(file?.external_subtitles ?? []), ...media.subtitles]
        .map((s) => languageName(s.language, i18n.language))
        .filter(Boolean),
    ),
  ];
  return (
    <div className="mx-auto mt-2 grid max-w-6xl gap-6 px-4 sm:grid-cols-3 sm:px-10">
      <InfoBlock label={t("detail.video")}>
        <div className="flex flex-wrap gap-1.5">
          {resolution ? <Badge tone="accent">{resolution}</Badge> : null}
          {video ? <Badge>{video.codec.toUpperCase()}</Badge> : null}
          {video?.hdr ? <Badge tone="accent">HDR</Badge> : null}
        </div>
      </InfoBlock>
      <InfoBlock label={t("detail.audio")}>{audio.join(", ") || "—"}</InfoBlock>
      <InfoBlock label={t("detail.subtitles")}>{subs.join(", ") || "—"}</InfoBlock>
    </div>
  );
}

function InfoBlock({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <p className="mb-1.5 text-xs font-semibold uppercase tracking-wider text-faint">{label}</p>
      <div className="text-sm text-fg/85">{children}</div>
    </div>
  );
}

export function MovieDetailPage() {
  const { t, i18n } = useTranslation();
  const { id } = useParams({ strict: false }) as { id: string };
  const { data: movie, isPending, isError } = useQuery(movieQuery(Number(id)));
  const setPlayed = useSetPlayed();

  if (isPending) {
    return (
      <Page className="mx-auto max-w-6xl px-4 pt-28 sm:px-10">
        <Skeleton className="mb-4 h-12 w-2/3" />
        <Skeleton className="h-24 w-full" />
      </Page>
    );
  }
  if (isError || !movie) return <NotFoundPage />;

  const duration =
    movie.file?.media?.duration_ms ??
    (movie.runtime_minutes ? movie.runtime_minutes * 60_000 : null);
  const progress = progressRatio(movie.position_ms, duration);

  return (
    <Page>
      <DetailHeader
        backdrop={movie.backdrop_path}
        poster={movie.poster_path}
        title={movie.title}
        overview={movie.overview}
        meta={
          <>
            {movie.year ? <span>{movie.year}</span> : null}
            {duration ? (
              <>
                <Dot />
                <span>{formatRuntime(duration, i18n.language)}</span>
              </>
            ) : null}
            {pickRating(movie.ratings) ? (
              <>
                <Dot />
                <Rating ratings={movie.ratings} className="font-semibold" />
              </>
            ) : null}
            {movie.played ? (
              <>
                <Dot />
                <span className="inline-flex items-center gap-1 text-success">
                  <Check className="size-4" /> {t("detail.watched")}
                </span>
              </>
            ) : null}
            <DownloadedMark kind="movie" id={movie.id} />
          </>
        }
        actions={
          <>
            <Link to="/watch/$kind/$id" params={{ kind: "movie", id }}>
              <Button variant="primary" size="lg">
                <Play className="size-5" fill="currentColor" />
                {movie.position_ms > 0 ? t("common.resume") : t("common.play")}
              </Button>
            </Link>
            {movie.position_ms > 0 ? (
              <Link to="/watch/$kind/$id" params={{ kind: "movie", id }} search={{ restart: true }}>
                <Button size="lg">
                  <RotateCcw className="size-5" />
                  {t("detail.restart")}
                </Button>
              </Link>
            ) : null}
            <Button
              size="lg"
              variant="ghost"
              loading={setPlayed.isPending}
              onClick={() =>
                setPlayed.mutate({ kind: "movie", id: movie.id, played: !movie.played })
              }
            >
              {movie.played ? <EyeOff className="size-5" /> : <Eye className="size-5" />}
              {movie.played ? t("detail.markUnwatched") : t("detail.markWatched")}
            </Button>
            <WatchlistButton movie={movie} />
            <DownloadMovieButton movie={movie} />
            <IdentifyButton kind="movies" id={movie.id} title={movie.title} year={movie.year} />
            {progress > 0 ? (
              <ProgressBar value={progress} className="w-full max-w-sm basis-full" />
            ) : null}
          </>
        }
      />
      <TechInfo file={movie.file} />
    </Page>
  );
}
