import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import { Layers } from "lucide-react";
import { motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { DetailHeader } from "../components/detail";
import { Img, PosterCard, PosterGrid } from "../components/media";
import { EmptyState, LoadError, Page, Skeleton } from "../components/ui";
import { ApiError, imageUrl, type Schemas } from "../lib/api";
import { collectionQuery, collectionsQuery } from "../lib/queries";
import { NotFoundPage } from "./NotFoundPage";

/** "2 of 3 movies", or "2 movies" while TMDB hasn't said how many the saga has (D101). */
function useSagaCount() {
  const { t } = useTranslation();
  return (count: number, total: number | null | undefined) =>
    total && total >= count ? t("saga.owned", { count, total }) : t("saga.count", { count });
}

function SagaCard({ saga, index }: { saga: Schemas["CollectionSummary"]; index: number }) {
  const sagaCount = useSagaCount();
  return (
    <motion.div
      initial={{ opacity: 0, y: 16 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.4, delay: Math.min(index, 12) * 0.03, ease: [0.22, 1, 0.36, 1] }}
    >
      <Link
        to="/sagas/$id"
        params={{ id: String(saga.id) }}
        className="group block outline-none"
        data-testid="saga-card"
      >
        <motion.div
          whileHover={{ y: -6, scale: 1.03 }}
          transition={{ type: "spring", stiffness: 320, damping: 22 }}
          className="relative aspect-[2/3] overflow-hidden rounded-[var(--radius-card)] bg-surface-2 shadow-lg ring-1 ring-white/5 transition-shadow duration-300 group-hover:shadow-2xl group-hover:shadow-black/60 group-hover:ring-accent/40 group-focus-visible:ring-2 group-focus-visible:ring-accent"
        >
          <Img
            src={imageUrl(saga.poster_path, "w342")}
            alt={saga.name}
            className="size-full object-cover"
            fallback={
              <div className="flex flex-col items-center gap-2 px-3 text-center">
                <Layers className="size-7 text-faint" />
                <span className="line-clamp-3 text-sm font-medium text-muted">{saga.name}</span>
              </div>
            }
          />
          <span className="absolute right-2 top-2 rounded-full bg-black/70 px-2 py-0.5 text-xs font-semibold backdrop-blur">
            {saga.movie_count}
          </span>
        </motion.div>
        <div className="mt-2.5 px-0.5">
          <p className="truncate text-sm font-medium text-fg/90 transition-colors group-hover:text-fg">
            {saga.name}
          </p>
          <p className="text-xs text-faint">{sagaCount(saga.movie_count, saga.total_count)}</p>
        </div>
      </Link>
    </motion.div>
  );
}

export function SagasPage() {
  const { t } = useTranslation();
  const query = useQuery(collectionsQuery);
  return (
    <Page className="mx-auto max-w-[1800px] px-4 pt-6 sm:px-10">
      <div className="mb-8">
        <h1 className="text-3xl font-bold tracking-tight">{t("nav.sagas")}</h1>
        {query.data ? (
          <p className="mt-1 text-sm text-muted">{t("saga.total", { count: query.data.length })}</p>
        ) : null}
      </div>
      {query.isPending ? (
        <PosterGrid>
          {Array.from({ length: 12 }, (_, i) => (
            <Skeleton key={i} className="aspect-[2/3]" />
          ))}
        </PosterGrid>
      ) : !query.data ? (
        <LoadError onRetry={query.refetch} />
      ) : query.data.length === 0 ? (
        <EmptyState icon={<Layers className="size-8" />} title={t("saga.none")}>
          <p>{t("saga.noneHint")}</p>
        </EmptyState>
      ) : (
        <PosterGrid>
          {query.data.map((saga, i) => (
            <SagaCard key={saga.id} saga={saga} index={i} />
          ))}
        </PosterGrid>
      )}
    </Page>
  );
}

export function SagaPage() {
  const { id } = useParams({ strict: false }) as { id: string };
  const { data: saga, isPending, error, refetch } = useQuery(collectionQuery(Number(id)));
  const sagaCount = useSagaCount();

  if (isPending) {
    return (
      <Page className="mx-auto max-w-6xl px-4 pt-28 sm:px-10">
        <Skeleton className="mb-4 h-12 w-2/3" />
        <Skeleton className="h-24 w-full" />
      </Page>
    );
  }
  if (error instanceof ApiError && error.status === 404) return <NotFoundPage />;
  if (!saga) {
    return (
      <Page className="px-4 pt-16">
        <LoadError onRetry={refetch} />
      </Page>
    );
  }
  return (
    <Page>
      <DetailHeader
        backdrop={saga.backdrop_path}
        poster={saga.poster_path}
        title={saga.name}
        overview={saga.overview}
        meta={
          <span className="inline-flex items-center gap-1.5">
            <Layers className="size-4" />
            {sagaCount(saga.movies.length, saga.total_count)}
          </span>
        }
        actions={null}
      />
      <div className="mx-auto max-w-6xl px-4 sm:px-10">
        <PosterGrid>
          {saga.movies.map((movie, i) => (
            <PosterCard key={movie.id} item={movie} kind="movie" index={i} />
          ))}
        </PosterGrid>
      </div>
    </Page>
  );
}
