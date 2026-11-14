import { useQuery } from "@tanstack/react-query";
import { Link, useLocalSearchParams } from "expo-router";
import { Layers } from "lucide-react-native";
import { useTranslation } from "react-i18next";
import { Pressable, ScrollView, Text, useWindowDimensions, View } from "react-native";
import { Hero } from "../../components/Detail";
import { COLORS, LoadError, Loading, Muted, Poster, Rating, Screen } from "../../components/ui";
import { imageUrl } from "../../lib/api";
import { setMovieList } from "../../lib/browse";
import { collectionQuery } from "../../lib/queries";

/** A saga (D101): the library's movies of a TMDB collection, in release order. */
export default function SagaScreen() {
  const { t } = useTranslation();
  const id = Number(useLocalSearchParams<{ id: string }>().id);
  const saga = useQuery(collectionQuery(id));
  const { width } = useWindowDimensions();
  const columns = Math.max(3, Math.floor(Math.min(width, 768) / 150));
  if (saga.isError) return <LoadError onRetry={() => void saga.refetch()} />;
  const s = saga.data;
  if (!s) return <Loading />;
  const count = s.movies.length;
  return (
    <Screen edges={[]}>
      <ScrollView contentContainerClassName="pb-10">
        <Hero backdrop={imageUrl(s.backdrop_path ?? s.poster_path, "w1280")}>
          <Text testID="title" className="text-2xl font-bold text-fg">
            {s.name}
          </Text>
          <View className="flex-row items-center gap-1.5">
            <Layers color={COLORS.muted} size={14} />
            <Muted>
              {s.total_count && s.total_count >= count
                ? t("saga.owned", { count, total: s.total_count })
                : t("saga.count", { count })}
            </Muted>
          </View>
        </Hero>
        <View className="w-full max-w-3xl gap-4 self-center p-4">
          {s.overview ? <Text className="text-base leading-6 text-muted">{s.overview}</Text> : null}
          <View className="flex-row flex-wrap gap-3">
            {s.movies.map((m) => (
              <Link key={m.id} href={`/movie/${m.id}`} asChild>
                <Pressable
                  onPressIn={() => setMovieList(s.movies.map((x) => x.id))}
                  testID={`saga-movie-${m.id}`}
                  style={{ width: `${100 / columns - 3}%` }}
                  className="active:opacity-70"
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
                  <Text numberOfLines={1} className="mt-1.5 text-xs font-medium text-fg">
                    {m.title}
                  </Text>
                  <View className="flex-row items-center gap-2">
                    {m.year ? <Text className="text-xs text-faint">{m.year}</Text> : null}
                    <Rating ratings={m.ratings} />
                  </View>
                </Pressable>
              </Link>
            ))}
          </View>
        </View>
      </ScrollView>
    </Screen>
  );
}
