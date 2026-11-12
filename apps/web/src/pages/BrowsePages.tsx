import { useInfiniteQuery } from "@tanstack/react-query";
import { Film, Tv } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { LoadMore, PosterCard, PosterGrid } from "../components/media";
import { EmptyState, Page, Select, Skeleton } from "../components/ui";
import { moviesQuery, type SortOrder, seriesListQuery } from "../lib/queries";

function useSort(key: string) {
  const [sort, setSort] = useState<SortOrder>(() => {
    try {
      return (localStorage.getItem(key) as SortOrder) || "added";
    } catch {
      return "added";
    }
  });
  const update = (value: SortOrder) => {
    setSort(value);
    try {
      localStorage.setItem(key, value);
    } catch {
      // private mode: the choice just isn't remembered
    }
  };
  return [sort, update] as const;
}

function BrowseHeader({
  title,
  total,
  sort,
  onSort,
}: {
  title: string;
  total?: number;
  sort: SortOrder;
  onSort: (s: SortOrder) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="mb-8 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
        {total !== undefined ? (
          <p className="mt-1 text-sm text-muted">{t("browse.count", { count: total })}</p>
        ) : null}
      </div>
      <Select
        value={sort}
        onChange={(e) => onSort(e.target.value as SortOrder)}
        aria-label={t("browse.sort")}
      >
        <option value="added">{t("browse.sortAdded")}</option>
        <option value="title">{t("browse.sortTitle")}</option>
        <option value="year">{t("browse.sortYear")}</option>
        <option value="rating">{t("browse.sortRating")}</option>
      </Select>
    </div>
  );
}

function GridSkeleton() {
  return (
    <PosterGrid>
      {Array.from({ length: 18 }, (_, i) => (
        <Skeleton key={i} className="aspect-[2/3]" />
      ))}
    </PosterGrid>
  );
}

export function MoviesPage() {
  const { t } = useTranslation();
  const [sort, setSort] = useSort("bv.sort.movies");
  const query = useInfiniteQuery(moviesQuery(sort));
  const items = query.data?.pages.flatMap((p) => p.items) ?? [];
  return (
    <Page className="mx-auto max-w-[1800px] px-4 pt-6 sm:px-10">
      <BrowseHeader
        title={t("nav.movies")}
        total={query.data?.pages[0]?.total}
        sort={sort}
        onSort={setSort}
      />
      {query.isPending ? (
        <GridSkeleton />
      ) : items.length === 0 ? (
        <EmptyState icon={<Film className="size-8" />} title={t("browse.noMovies")} />
      ) : (
        <PosterGrid>
          {items.map((movie, i) => (
            <PosterCard key={movie.id} item={movie} kind="movie" index={i % 60} />
          ))}
        </PosterGrid>
      )}
      <LoadMore
        active={!!query.hasNextPage && !query.isFetchingNextPage}
        onVisible={() => query.fetchNextPage()}
      />
    </Page>
  );
}

export function SeriesListPage() {
  const { t } = useTranslation();
  const [sort, setSort] = useSort("bv.sort.series");
  const query = useInfiniteQuery(seriesListQuery(sort));
  const items = query.data?.pages.flatMap((p) => p.items) ?? [];
  return (
    <Page className="mx-auto max-w-[1800px] px-4 pt-6 sm:px-10">
      <BrowseHeader
        title={t("nav.series")}
        total={query.data?.pages[0]?.total}
        sort={sort}
        onSort={setSort}
      />
      {query.isPending ? (
        <GridSkeleton />
      ) : items.length === 0 ? (
        <EmptyState icon={<Tv className="size-8" />} title={t("browse.noSeries")} />
      ) : (
        <PosterGrid>
          {items.map((series, i) => (
            <PosterCard key={series.id} item={series} kind="series" index={i % 60} />
          ))}
        </PosterGrid>
      )}
      <LoadMore
        active={!!query.hasNextPage && !query.isFetchingNextPage}
        onVisible={() => query.fetchNextPage()}
      />
    </Page>
  );
}
