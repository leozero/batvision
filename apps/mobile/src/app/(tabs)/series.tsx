import { parseSort, type WatchedFilter } from "@batvision/core/sort";
import { useInfiniteQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Catalog, type Sorting } from "../../components/Catalog";
import { Screen } from "../../components/ui";
import { imageUrl } from "../../lib/api";
import { seriesListQuery } from "../../lib/queries";

export default function SeriesList() {
  const { t } = useTranslation();
  const [sorting, setSorting] = useState<Sorting>(() => parseSort(null, null));
  const [filter, setFilter] = useState<WatchedFilter>("all");
  const q = useInfiniteQuery(seriesListQuery(sorting.sort, sorting.direction, filter));
  const items = (q.data?.pages ?? [])
    .flatMap((p) => p.items)
    .map((s) => ({
      key: String(s.id),
      href: `/series/${s.id}`,
      title: s.title,
      year: s.year,
      poster: imageUrl(s.poster_path, "w342"),
      ratings: s.ratings,
      badges: {
        kind: "series" as const,
        id: s.id,
        watched: s.episode_count > 0 && s.unplayed_count === 0,
        inWatchlist: s.in_watchlist,
      },
    }));
  return (
    <Screen>
      <Catalog
        title={t("nav.series")}
        items={items}
        loading={q.isPending}
        refreshing={q.isRefetching}
        onRefresh={() => void q.refetch()}
        onEnd={() => q.hasNextPage && !q.isFetchingNextPage && void q.fetchNextPage()}
        sorting={sorting}
        onSort={setSorting}
        filter={filter}
        onFilter={setFilter}
        empty={t("browse.noSeries")}
      />
    </Screen>
  );
}
