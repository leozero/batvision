import { useInfiniteQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Catalog } from "../../components/Catalog";
import { Screen } from "../../components/ui";
import { imageUrl } from "../../lib/api";
import { type SortOrder, seriesListQuery } from "../../lib/queries";

export default function SeriesList() {
  const { t } = useTranslation();
  const [sort, setSort] = useState<SortOrder>("added");
  const q = useInfiniteQuery(seriesListQuery(sort));
  const items = (q.data?.pages ?? [])
    .flatMap((p) => p.items)
    .map((s) => ({
      key: String(s.id),
      href: `/series/${s.id}`,
      title: s.title,
      year: s.year,
      poster: imageUrl(s.poster_path, "w342"),
      ratings: s.ratings,
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
        sort={sort}
        onSort={setSort}
        empty={t("browse.noSeries")}
      />
    </Screen>
  );
}
