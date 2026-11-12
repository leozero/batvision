import { parseSort } from "@batvision/core/sort";
import { useInfiniteQuery } from "@tanstack/react-query";
import { useLocalSearchParams } from "expo-router";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Catalog, type Sorting } from "../../components/Catalog";
import { Screen } from "../../components/ui";
import { imageUrl } from "../../lib/api";
import { moviesQuery } from "../../lib/queries";

export default function Movies() {
  const { t } = useTranslation();
  // "See all" on the top-rated rail opens this tab sorted by rating.
  const { sort: linked } = useLocalSearchParams<{ sort?: string }>();
  const [sorting, setSorting] = useState<Sorting>(() => parseSort(linked, null));
  useEffect(() => {
    if (linked) setSorting(parseSort(linked, null));
  }, [linked]);
  const q = useInfiniteQuery(moviesQuery(sorting.sort, sorting.direction));
  const items = (q.data?.pages ?? [])
    .flatMap((p) => p.items)
    .map((m) => ({
      key: String(m.id),
      href: `/movie/${m.id}`,
      title: m.title,
      year: m.year,
      poster: imageUrl(m.poster_path, "w342"),
      progress: m.duration_ms && m.position_ms ? m.position_ms / m.duration_ms : undefined,
      ratings: m.ratings,
    }));
  return (
    <Screen>
      <Catalog
        title={t("nav.movies")}
        items={items}
        loading={q.isPending}
        refreshing={q.isRefetching}
        onRefresh={() => void q.refetch()}
        onEnd={() => q.hasNextPage && !q.isFetchingNextPage && void q.fetchNextPage()}
        sorting={sorting}
        onSort={setSorting}
        empty={t("browse.noMovies")}
      />
    </Screen>
  );
}
