import { useInfiniteQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Catalog } from "../../components/Catalog";
import { Screen } from "../../components/ui";
import { imageUrl } from "../../lib/api";
import { moviesQuery, type SortOrder } from "../../lib/queries";

export default function Movies() {
  const { t } = useTranslation();
  const [sort, setSort] = useState<SortOrder>("added");
  const q = useInfiniteQuery(moviesQuery(sort));
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
        sort={sort}
        onSort={setSort}
        empty={t("browse.noMovies")}
      />
    </Screen>
  );
}
