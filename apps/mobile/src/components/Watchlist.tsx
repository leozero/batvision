import { languageName } from "@batvision/core/format";
import { useQuery } from "@tanstack/react-query";
import { router } from "expo-router";
import { Bookmark, BookmarkCheck, Search } from "lucide-react-native";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Pressable, Text, View } from "react-native";
import type { Schemas } from "../lib/api";
import { meQuery, preferencesQuery, useWatchlist } from "../lib/queries";
import { ChoiceList, Sheet } from "./Sheet";
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

const same = (a: string, b: string) =>
  a === b || (a === "fra" && b === "fre") || (a === "fre" && b === "fra");

/** Adds a movie: with several audio languages, asks which one to download (D40). */
export function MovieWatchlistButton({ movie }: { movie: Schemas["MovieDetail"] }) {
  const { t, i18n } = useTranslation();
  const watchlist = useWatchlist();
  const { data: prefs } = useQuery(preferencesQuery);
  const [open, setOpen] = useState(false);
  const languages = [
    ...new Set(
      (movie.file?.media?.audio ?? []).map((a) => a.language).filter((l): l is string => !!l),
    ),
  ];
  const preferred = languages.find((l) => prefs?.audio_language && same(l, prefs.audio_language));
  const [audio, setAudio] = useState<string>("");
  const add = (audioLanguage: string | null) =>
    watchlist.mutate(
      { kind: "movie", id: movie.id, add: true, audioLanguage },
      { onSuccess: () => setOpen(false) },
    );
  if (movie.in_watchlist) {
    return (
      <Button
        testID="watchlist"
        loading={watchlist.isPending}
        icon={<BookmarkCheck color={COLORS.accent} size={18} />}
        onPress={() => watchlist.mutate({ kind: "movie", id: movie.id, add: false })}
      >
        {t("watchlist.inWatchlist")}
      </Button>
    );
  }
  return (
    <>
      <Button
        testID="watchlist"
        loading={watchlist.isPending && !open}
        icon={<Bookmark color={COLORS.fg} size={18} />}
        onPress={() => {
          if (languages.length > 1) {
            setAudio(preferred ?? "");
            setOpen(true);
          } else add(languages[0] ?? null);
        }}
      >
        {t("watchlist.addLong")}
      </Button>
      <Sheet
        open={open}
        onClose={() => setOpen(false)}
        title={t("downloads.audioLanguage")}
        description={t("watchlist.intro")}
      >
        <ChoiceList
          value={audio}
          onChange={setAudio}
          options={[
            { value: "", label: t("downloads.defaultTrack") },
            ...languages.map((code) => ({
              value: code,
              label: languageName(code, i18n.language) ?? code,
            })),
          ]}
        />
        <ErrorText>{watchlist.error ? String(watchlist.error.message) : null}</ErrorText>
        <Button
          testID="watchlist-add"
          variant="primary"
          loading={watchlist.isPending}
          onPress={() => add(audio || null)}
        >
          {t("watchlist.addLong")}
        </Button>
      </Sheet>
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
