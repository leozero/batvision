import { useQuery } from "@tanstack/react-query";
import { router } from "expo-router";
import { Bookmark, BookmarkCheck, Search } from "lucide-react-native";
import { useTranslation } from "react-i18next";
import { Pressable, Text, View } from "react-native";
import type { Schemas } from "../lib/api";
import { meQuery, useWatchlist } from "../lib/queries";
import { Button, COLORS, ErrorText } from "./ui";

/** The magnifier at the top of Home, Movies and Series (search left the tab bar). */
export function SearchButton() {
  const { t } = useTranslation();
  return (
    <Pressable
      testID="open-search"
      accessibilityRole="button"
      accessibilityLabel={t("nav.search")}
      hitSlop={8}
      onPress={() => router.push("/search")}
      className="size-10 items-center justify-center rounded-full bg-surface-2 active:opacity-70"
    >
      <Search color={COLORS.fg} size={20} />
    </Pressable>
  );
}

/** The account, next to search: the user's initial, opening their profile and settings. */
export function ProfileButton() {
  const { t } = useTranslation();
  const { data: me } = useQuery(meQuery);
  return (
    <Pressable
      testID="open-profile"
      accessibilityRole="button"
      accessibilityLabel={t("nav.account")}
      hitSlop={8}
      onPress={() => router.push("/settings")}
      className="size-10 items-center justify-center rounded-full bg-accent active:opacity-70"
    >
      <Text className="text-base font-bold text-accent-ink">
        {me?.username.charAt(0).toUpperCase() ?? ""}
      </Text>
    </Pressable>
  );
}

/** Top right of Home, Movies and Series: search and the account. */
export function HeaderButtons() {
  return (
    <View className="flex-row items-center gap-2">
      <SearchButton />
      <ProfileButton />
    </View>
  );
}

/** Adds a movie in one tap: every audio track is downloaded (D40), the preferred
 * language plays first. */
export function MovieWatchlistButton({ movie }: { movie: Schemas["MovieDetail"] }) {
  const { t } = useTranslation();
  const watchlist = useWatchlist();
  return (
    <>
      <Button
        testID="watchlist"
        loading={watchlist.isPending}
        icon={
          movie.in_watchlist ? (
            <BookmarkCheck color={COLORS.accent} size={18} />
          ) : (
            <Bookmark color={COLORS.fg} size={18} />
          )
        }
        onPress={() => watchlist.mutate({ kind: "movie", id: movie.id, add: !movie.in_watchlist })}
      >
        {movie.in_watchlist ? t("watchlist.inWatchlist") : t("watchlist.addLong")}
      </Button>
      <ErrorText>{watchlist.error ? String(watchlist.error.message) : null}</ErrorText>
    </>
  );
}

/** Adds a series: its next unwatched episodes download, no question asked (D73). */
export function SeriesWatchlistButton({ series }: { series: Schemas["SeriesDetail"] }) {
  const { t } = useTranslation();
  const watchlist = useWatchlist();
  return (
    <Button
      testID="watchlist"
      loading={watchlist.isPending}
      icon={
        series.in_watchlist ? (
          <BookmarkCheck color={COLORS.accent} size={18} />
        ) : (
          <Bookmark color={COLORS.fg} size={18} />
        )
      }
      onPress={() => watchlist.mutate({ kind: "series", id: series.id, add: !series.in_watchlist })}
    >
      {series.in_watchlist ? t("watchlist.inWatchlist") : t("watchlist.addLong")}
    </Button>
  );
}
