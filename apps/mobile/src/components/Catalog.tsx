import { Link } from "expo-router";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { FlatList, Pressable, RefreshControl, Text, useWindowDimensions, View } from "react-native";
import type { Schemas } from "../lib/api";
import type { SortOrder } from "../lib/queries";
import { EmptyState, Loading, Poster, Rating } from "./ui";

export type CatalogItem = {
  key: string;
  href: string;
  title: string;
  year?: number | null;
  poster?: string;
  progress?: number;
  ratings?: Schemas["Rating"][];
};

const SORTS: SortOrder[] = ["added", "title", "year", "rating"];
const SORT_LABEL: Record<SortOrder, string> = {
  added: "browse.sortAdded",
  title: "browse.sortTitle",
  year: "browse.sortYear",
  rating: "browse.sortRating",
};

/** A poster grid: 3 columns on a phone, more on a tablet (D72). */
export function Catalog(props: {
  title: string;
  items: CatalogItem[];
  loading: boolean;
  refreshing: boolean;
  onRefresh: () => void;
  onEnd: () => void;
  sort: SortOrder;
  onSort: (sort: SortOrder) => void;
  empty: string;
  header?: ReactNode;
}) {
  const { t } = useTranslation();
  const { width } = useWindowDimensions();
  const columns = Math.max(3, Math.floor(width / 150));
  if (props.loading) return <Loading />;
  return (
    <FlatList
      key={columns}
      data={props.items}
      numColumns={columns}
      keyExtractor={(i) => i.key}
      onEndReached={props.onEnd}
      onEndReachedThreshold={0.6}
      refreshControl={<RefreshControl refreshing={props.refreshing} onRefresh={props.onRefresh} />}
      contentContainerClassName="gap-4 px-3 pb-8"
      columnWrapperClassName="gap-3"
      ListHeaderComponent={
        <View className="gap-3 px-1 pt-4">
          <Text className="text-2xl font-bold text-fg">{props.title}</Text>
          <View className="flex-row flex-wrap gap-2">
            {SORTS.map((s) => (
              <Pressable
                key={s}
                onPress={() => props.onSort(s)}
                className={`rounded-full px-3 py-1.5 ${s === props.sort ? "bg-accent" : "bg-surface-2"}`}
              >
                <Text
                  className={`text-xs font-semibold ${s === props.sort ? "text-accent-ink" : "text-muted"}`}
                >
                  {t(SORT_LABEL[s])}
                </Text>
              </Pressable>
            ))}
          </View>
          {props.header}
        </View>
      }
      ListEmptyComponent={<EmptyState title={props.empty} />}
      renderItem={({ item }) => (
        <Link href={item.href as never} asChild>
          <Pressable style={{ width: `${100 / columns - 3}%` }} className="active:opacity-70">
            <Poster uri={item.poster} progress={item.progress} />
            <Text numberOfLines={1} className="mt-1.5 text-xs font-medium text-fg">
              {item.title}
            </Text>
            <View className="flex-row items-center gap-2">
              {item.year ? <Text className="text-xs text-faint">{item.year}</Text> : null}
              <Rating ratings={item.ratings} />
            </View>
          </Pressable>
        </Link>
      )}
    />
  );
}
