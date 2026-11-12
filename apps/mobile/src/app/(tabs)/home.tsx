import { useQuery } from "@tanstack/react-query";
import { Link } from "expo-router";
import { useTranslation } from "react-i18next";
import {
  FlatList,
  Platform,
  Pressable,
  RefreshControl,
  ScrollView,
  Text,
  View,
} from "react-native";
import { UpdateBanner } from "../../components/UpdateBanner";
import { EmptyState, Loading, Poster, Screen } from "../../components/ui";
import { SearchButton } from "../../components/Watchlist";
import { imageUrl, type Schemas } from "../../lib/api";
import { homeQuery } from "../../lib/queries";

type Card = {
  key: string;
  href: string;
  title: string;
  subtitle?: string | null;
  poster?: string | null;
  progress?: number;
};

function Row({
  title,
  items,
  action,
}: {
  title: string;
  items: Card[];
  /** A link next to the title ("See all"). */
  action?: { label: string; href: string; testID?: string };
}) {
  if (items.length === 0) return null;
  return (
    <View className="gap-3">
      <View className="flex-row items-baseline justify-between px-4">
        <Text className="text-lg font-semibold text-fg">{title}</Text>
        {action ? (
          <Link href={action.href as never} asChild>
            <Pressable testID={action.testID} hitSlop={8}>
              <Text className="text-sm text-accent">{action.label}</Text>
            </Pressable>
          </Link>
        ) : null}
      </View>
      <FlatList
        horizontal
        data={items}
        keyExtractor={(i) => i.key}
        showsHorizontalScrollIndicator={false}
        contentContainerClassName="gap-3 px-4"
        renderItem={({ item }) => (
          <Link href={item.href as never} asChild>
            <Pressable className="w-32 active:opacity-70">
              <Poster uri={item.poster} progress={item.progress} />
              <Text numberOfLines={1} className="mt-2 text-sm font-medium text-fg">
                {item.title}
              </Text>
              {item.subtitle ? (
                <Text numberOfLines={1} className="text-xs text-faint">
                  {item.subtitle}
                </Text>
              ) : null}
            </Pressable>
          </Link>
        )}
      />
    </View>
  );
}

function homeCard(item: Schemas["HomeItem"]): Card {
  const episode = item.kind === "episode";
  return {
    key: `${item.kind}-${item.id}`,
    href: episode ? `/series/${item.series_id}` : `/movie/${item.id}`,
    title: item.title,
    subtitle: item.subtitle,
    poster: imageUrl(item.poster_path, "w342"),
    progress: item.duration_ms ? item.position_ms / item.duration_ms : undefined,
  };
}

export default function HomeScreen() {
  const { t } = useTranslation();
  const home = useQuery(homeQuery);
  if (home.isPending) return <Loading />;
  const data = home.data;
  const empty =
    data &&
    !data.continue_watching.length &&
    !data.next_up.length &&
    !data.recent_movies.length &&
    !data.recent_series.length;
  return (
    <Screen>
      <ScrollView
        contentContainerClassName="gap-8 pb-8"
        refreshControl={
          <RefreshControl refreshing={home.isRefetching} onRefresh={() => void home.refetch()} />
        }
      >
        <View className="flex-row items-center justify-between px-4 pt-4">
          <Text className="text-3xl font-bold text-accent">BatVision</Text>
          <SearchButton />
        </View>
        {/* iOS: TestFlight and the App Store update the app (D90). */}
        {Platform.OS === "android" ? <UpdateBanner /> : null}
        {empty ? <EmptyState title={t("home.emptyTitle")} /> : null}
        <Row
          title={t("home.continueWatching")}
          items={(data?.continue_watching ?? []).map(homeCard)}
        />
        <Row title={t("home.nextUp")} items={(data?.next_up ?? []).map(homeCard)} />
        <Row
          title={t("home.recentMovies")}
          items={(data?.recent_movies ?? []).map((m) => ({
            key: `movie-${m.id}`,
            href: `/movie/${m.id}`,
            title: m.title,
            subtitle: m.year ? String(m.year) : null,
            poster: imageUrl(m.poster_path, "w342"),
          }))}
        />
        <Row
          title={t("home.topRatedMovies")}
          action={{ label: t("home.seeAll"), href: "/movies?sort=rating", testID: "top-rated-all" }}
          items={(data?.top_rated_movies ?? []).map((m) => ({
            key: `top-${m.id}`,
            href: `/movie/${m.id}`,
            title: m.title,
            subtitle: m.year ? String(m.year) : null,
            poster: imageUrl(m.poster_path, "w342"),
          }))}
        />
        <Row
          title={t("home.recentSeries")}
          items={(data?.recent_series ?? []).map((s) => ({
            key: `series-${s.id}`,
            href: `/series/${s.id}`,
            title: s.title,
            subtitle: s.year ? String(s.year) : null,
            poster: imageUrl(s.poster_path, "w342"),
          }))}
        />
      </ScrollView>
    </Screen>
  );
}
