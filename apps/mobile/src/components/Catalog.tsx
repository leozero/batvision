import {
  directionLabelKey,
  flipDirection,
  naturalDirection,
  SORT_ORDERS,
  type SortDirection,
  type SortOrder,
} from "@batvision/core/sort";
import { Link } from "expo-router";
import { ArrowDownWideNarrow, ArrowUpNarrowWide } from "lucide-react-native";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { FlatList, Pressable, RefreshControl, Text, useWindowDimensions, View } from "react-native";
import type { Schemas } from "../lib/api";
import { COLORS, EmptyState, Loading, Poster, Rating } from "./ui";
import { SearchButton } from "./Watchlist";

export type Sorting = { sort: SortOrder; direction: SortDirection };

export type CatalogItem = {
  key: string;
  href: string;
  title: string;
  year?: number | null;
  poster?: string;
  progress?: number;
  ratings?: Schemas["Rating"][];
};

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
  sorting: Sorting;
  onSort: (sorting: Sorting) => void;
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
          <View className="flex-row items-center justify-between">
            <Text className="text-2xl font-bold text-fg">{props.title}</Text>
            <SearchButton />
          </View>
          <View className="flex-row flex-wrap gap-2">
            {SORT_ORDERS.map((s) => {
              const active = s === props.sorting.sort;
              return (
                <Pressable
                  key={s}
                  testID={`sort-${s}`}
                  accessibilityState={{ selected: active }}
                  // A new sort starts in its natural direction (D75).
                  onPress={() => props.onSort({ sort: s, direction: naturalDirection(s) })}
                  className={`rounded-full px-3 py-1.5 ${active ? "bg-accent" : "bg-surface-2"}`}
                >
                  <Text
                    className={`text-xs font-semibold ${active ? "text-accent-ink" : "text-muted"}`}
                  >
                    {t(SORT_LABEL[s])}
                  </Text>
                </Pressable>
              );
            })}
          </View>
          <Pressable
            testID="sort-direction"
            accessibilityRole="button"
            accessibilityHint={t("browse.reverse")}
            onPress={() =>
              props.onSort({ ...props.sorting, direction: flipDirection(props.sorting.direction) })
            }
            className="flex-row items-center gap-1.5 self-start rounded-full bg-surface-2 px-3 py-1.5 active:opacity-70"
          >
            {props.sorting.direction === "asc" ? (
              <ArrowUpNarrowWide color={COLORS.fg} size={14} />
            ) : (
              <ArrowDownWideNarrow color={COLORS.fg} size={14} />
            )}
            <Text className="text-xs font-semibold text-fg">
              {t(directionLabelKey(props.sorting.sort, props.sorting.direction))}
            </Text>
          </Pressable>
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
