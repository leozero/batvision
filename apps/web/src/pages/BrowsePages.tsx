import {
  directionLabelKey,
  flipDirection,
  naturalDirection,
  parseSort,
  type SortDirection,
  type SortOrder,
} from "@batvision/core/sort";
import { useInfiniteQuery } from "@tanstack/react-query";
import { ArrowDownWideNarrow, ArrowUpNarrowWide, Film, Tv } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { LoadMore, PosterCard, PosterGrid } from "../components/media";
import { Button, EmptyState, Page, Select, Skeleton } from "../components/ui";
import { moviesQuery, seriesListQuery } from "../lib/queries";

type Sorting = { sort: SortOrder; direction: SortDirection };

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // private mode: the choice just isn't remembered
  }
}

/** The page's sort and direction, remembered per page (D75). `initial` comes from a link. */
function useSort(key: string, initial?: SortOrder) {
  const [sorting, setSorting] = useState<Sorting>(() => {
    if (initial) {
      const s = { sort: initial, direction: naturalDirection(initial) };
      write(key, s.sort);
      write(`${key}.order`, s.direction);
      return s;
    }
    return parseSort(read(key), read(`${key}.order`));
  });
  const update = (next: Sorting) => {
    setSorting(next);
    write(key, next.sort);
    write(`${key}.order`, next.direction);
  };
  return [sorting, update] as const;
}

function BrowseHeader({
  title,
  total,
  sorting,
  onSort,
}: {
  title: string;
  total?: number;
  sorting: Sorting;
  onSort: (s: Sorting) => void;
}) {
  const { t } = useTranslation();
  const { sort, direction } = sorting;
  const Arrow = direction === "asc" ? ArrowUpNarrowWide : ArrowDownWideNarrow;
  return (
    <div className="mb-8 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
        {total !== undefined ? (
          <p className="mt-1 text-sm text-muted">{t("browse.count", { count: total })}</p>
        ) : null}
      </div>
      <div className="flex items-center gap-2">
        <Select
          value={sort}
          onChange={(e) => {
            // A new sort starts in its natural direction.
            const next = e.target.value as SortOrder;
            onSort({ sort: next, direction: naturalDirection(next) });
          }}
          aria-label={t("browse.sort")}
        >
          <option value="added">{t("browse.sortAdded")}</option>
          <option value="title">{t("browse.sortTitle")}</option>
          <option value="year">{t("browse.sortYear")}</option>
          <option value="rating">{t("browse.sortRating")}</option>
        </Select>
        <Button
          variant="ghost"
          title={t("browse.reverse")}
          data-testid="sort-direction"
          onClick={() => onSort({ sort, direction: flipDirection(direction) })}
        >
          <Arrow className="size-4" />
          {t(directionLabelKey(sort, direction))}
        </Button>
      </div>
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

export function MoviesPage({ initialSort }: { initialSort?: SortOrder }) {
  const { t } = useTranslation();
  const [sorting, setSorting] = useSort("bv.sort.movies", initialSort);
  const query = useInfiniteQuery(moviesQuery(sorting.sort, sorting.direction));
  const items = query.data?.pages.flatMap((p) => p.items) ?? [];
  return (
    <Page className="mx-auto max-w-[1800px] px-4 pt-6 sm:px-10">
      <BrowseHeader
        title={t("nav.movies")}
        total={query.data?.pages[0]?.total}
        sorting={sorting}
        onSort={setSorting}
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
  const [sorting, setSorting] = useSort("bv.sort.series");
  const query = useInfiniteQuery(seriesListQuery(sorting.sort, sorting.direction));
  const items = query.data?.pages.flatMap((p) => p.items) ?? [];
  return (
    <Page className="mx-auto max-w-[1800px] px-4 pt-6 sm:px-10">
      <BrowseHeader
        title={t("nav.series")}
        total={query.data?.pages[0]?.total}
        sorting={sorting}
        onSort={setSorting}
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
