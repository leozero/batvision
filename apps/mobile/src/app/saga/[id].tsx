import { useQuery } from "@tanstack/react-query";
import { Link, useLocalSearchParams } from "expo-router";
import { Layers } from "lucide-react-native";
import { useTranslation } from "react-i18next";
import { ScrollView, Text, View } from "react-native";
import { Hero } from "../../components/Detail";
import { Pressable } from "../../components/Pressable";
import { COLORS, LoadError, Loading, Muted, Poster, Rating, Screen } from "../../components/ui";
import { imageUrl, type Schemas } from "../../lib/api";
import { setMovieList } from "../../lib/browse";
import { useMeasuredGrid } from "../../lib/layout";
import { collectionQuery } from "../../lib/queries";
import { ui } from "../../lib/scale";
import { TV_EDGE, tvGap, tvTail } from "../../lib/tv";

type SagaEntry =
  | { kind: "owned"; movie: Schemas["MovieSummary"] }
  | { kind: "missing"; movie: Schemas["MissingMovie"] };

/** The library's movies with the missing ones slotted in, all in release order. */
function sagaEntries(saga: Schemas["CollectionDetail"]): SagaEntry[] {
  // A missing movie at `position` p goes after the library's first p movies.
  const owned = saga.movies.map((movie, i) => ({ kind: "owned" as const, movie, at: i + 0.5 }));
  const missing = saga.missing.map((movie) => ({
    kind: "missing" as const,
    movie,
    at: movie.position,
  }));
  return [...missing, ...owned].sort((a, b) => a.at - b.at);
}

/**
 * A saga (D101): the movies of a TMDB collection in release order, the ones the library
 * doesn't have greyed out.
 */
export default function SagaScreen() {
  const { t } = useTranslation();
  const id = Number(useLocalSearchParams<{ id: string }>().id);
  const saga = useQuery(collectionQuery(id));
  const grid = useMeasuredGrid(32);
  if (saga.isError) return <LoadError onRetry={() => void saga.refetch()} />;
  const s = saga.data;
  if (!s) return <Loading />;
  const count = s.movies.length;
  return (
    <Screen edges={[]}>
      <ScrollView fadingEdgeLength={TV_EDGE} contentContainerClassName={tvTail("pb-10")}>
        <Hero backdrop={imageUrl(s.backdrop_path ?? s.poster_path, "w1280")}>
          <Text testID="title" className="text-2xl font-bold text-fg">
            {s.name}
          </Text>
          <View className="flex-row items-center gap-1.5">
            <Layers color={COLORS.muted} size={ui(14)} />
            <Muted>
              {s.total_count && s.total_count >= count
                ? t("saga.owned", { count, total: s.total_count })
                : t("saga.count", { count })}
            </Muted>
          </View>
        </Hero>
        <View className="w-full max-w-3xl gap-4 self-center p-4">
          {s.overview ? <Text className="text-base leading-6 text-muted">{s.overview}</Text> : null}
          <View onLayout={grid.onLayout} className="flex-row flex-wrap gap-3">
            {sagaEntries(s).map((entry) => {
              if (entry.kind === "missing") {
                const m = entry.movie;
                return (
                  <View
                    key={`tmdb-${m.tmdb_id}`}
                    testID={`saga-missing-${m.tmdb_id}`}
                    style={{ width: grid.itemWidth, opacity: 0.4, filter: [{ grayscale: 1 }] }}
                  >
                    <Poster uri={imageUrl(m.poster_path, "w342")} />
                    <Text
                      numberOfLines={1}
                      className={`${tvGap("mt-1.5")} text-xs font-medium text-muted`}
                    >
                      {m.title}
                    </Text>
                    <Text numberOfLines={1} className="text-xs text-faint">
                      {m.year ? `${m.year} · ` : ""}
                      {t("saga.missing")}
                    </Text>
                  </View>
                );
              }
              const m = entry.movie;
              return (
                <Link key={m.id} href={`/movie/${m.id}`} asChild>
                  <Pressable
                    onPressIn={() => setMovieList(s.movies.map((x) => x.id))}
                    testID={`saga-movie-${m.id}`}
                    style={{ width: grid.itemWidth }}
                    className="active:opacity-70"
                    ringOnChild
                  >
                    <Poster
                      badges={{
                        kind: "movie",
                        id: m.id,
                        watched: m.played,
                        inWatchlist: m.in_watchlist,
                      }}
                      uri={imageUrl(m.poster_path, "w342")}
                      progress={
                        m.duration_ms && m.position_ms ? m.position_ms / m.duration_ms : undefined
                      }
                    />
                    <Text
                      numberOfLines={1}
                      className={`${tvGap("mt-1.5")} text-xs font-medium text-fg`}
                    >
                      {m.title}
                    </Text>
                    <View className="flex-row items-center gap-2">
                      {m.year ? <Text className="text-xs text-faint">{m.year}</Text> : null}
                      <Rating ratings={m.ratings} />
                    </View>
                  </Pressable>
                </Link>
              );
            })}
          </View>
        </View>
      </ScrollView>
    </Screen>
  );
}
