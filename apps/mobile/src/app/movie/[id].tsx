import { formatRuntime } from "@batvision/core/format";
import { useQuery } from "@tanstack/react-query";
import { router, useLocalSearchParams } from "expo-router";
import { Check, Download, Play } from "lucide-react-native";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ScrollView, Text, View } from "react-native";
import { Hero, LocalMark, WatchedMark } from "../../components/Detail";
import { Button, COLORS, ErrorText, Loading, Muted, Rating, Screen } from "../../components/ui";
import { imageUrl } from "../../lib/api";
import { movieQuery, useManualDownload, useSetPlayed } from "../../lib/queries";
import { session } from "../../lib/session";
import { useLocalItem } from "../../lib/sync";

export default function MovieScreen() {
  const { t, i18n } = useTranslation();
  const id = Number(useLocalSearchParams<{ id: string }>().id);
  const movie = useQuery(movieQuery(id));
  const download = useManualDownload();
  const played = useSetPlayed();
  const local = useLocalItem("movie", id);
  const [requested, setRequested] = useState(false);
  if (movie.isPending) return <Loading />;
  const m = movie.data;
  if (!m) return <Loading />;
  const resume = m.position_ms > 0 && !m.played;
  return (
    <Screen edges={[]}>
      <ScrollView contentContainerClassName="pb-10">
        <Hero backdrop={imageUrl(m.backdrop_path ?? m.poster_path, "w1280")}>
          <Text testID="title" className="text-2xl font-bold text-fg">
            {m.title}
          </Text>
          <View className="flex-row flex-wrap items-center gap-3">
            {m.year ? <Muted>{String(m.year)}</Muted> : null}
            {m.runtime_minutes ? (
              <Muted>{formatRuntime(m.runtime_minutes * 60_000, i18n.language) ?? ""}</Muted>
            ) : null}
            <Rating ratings={m.ratings} />
            {m.played ? <WatchedMark /> : null}
            <LocalMark kind="movie" id={id} />
          </View>
        </Hero>
        <View className="gap-4 p-4">
          <Button
            testID="play"
            variant="primary"
            icon={<Play color={COLORS.accentInk} size={18} />}
            onPress={() => router.push(`/watch/movie/${id}`)}
          >
            {resume ? t("common.resume") : t("common.play")}
          </Button>
          {!local ? (
            <Button
              testID="download"
              icon={<Download color={COLORS.fg} size={18} />}
              loading={download.isPending}
              disabled={requested}
              onPress={() => {
                const deviceId = session().deviceId;
                if (deviceId == null) return;
                download.mutate(
                  { device_id: deviceId, kind: "movie", id },
                  { onSuccess: () => setRequested(true) },
                );
              }}
            >
              {requested ? t("mobile.requested") : t("mobile.downloadHere")}
            </Button>
          ) : null}
          <ErrorText>{download.error ? String(download.error.message) : null}</ErrorText>
          <Button
            icon={<Check color={COLORS.fg} size={18} />}
            onPress={() => played.mutate({ kind: "movie", id, played: !m.played })}
          >
            {m.played ? t("detail.markUnwatched") : t("detail.markWatched")}
          </Button>
          {m.overview ? <Text className="text-base leading-6 text-muted">{m.overview}</Text> : null}
        </View>
      </ScrollView>
    </Screen>
  );
}
