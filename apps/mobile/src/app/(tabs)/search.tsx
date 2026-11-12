import { useQuery } from "@tanstack/react-query";
import { Link } from "expo-router";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { FlatList, Pressable, Text, View } from "react-native";
import { EmptyState, Input, Poster, Screen } from "../../components/ui";
import { imageUrl } from "../../lib/api";
import { searchQuery } from "../../lib/queries";

export default function SearchScreen() {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  const [q, setQ] = useState("");
  useEffect(() => {
    const timer = setTimeout(() => setQ(text.trim()), 300);
    return () => clearTimeout(timer);
  }, [text]);
  const results = useQuery(searchQuery(q));
  const items = [
    ...(results.data?.movies ?? []).map((m) => ({
      key: `m${m.id}`,
      href: `/movie/${m.id}`,
      title: m.title,
      year: m.year,
      poster: m.poster_path,
    })),
    ...(results.data?.series ?? []).map((s) => ({
      key: `s${s.id}`,
      href: `/series/${s.id}`,
      title: s.title,
      year: s.year,
      poster: s.poster_path,
    })),
  ];
  return (
    <Screen>
      <View className="gap-3 p-4">
        <Text className="text-2xl font-bold text-fg">{t("nav.search")}</Text>
        <Input
          testID="search"
          value={text}
          onChangeText={setText}
          placeholder={t("search.placeholder")}
          autoFocus
          returnKeyType="search"
        />
      </View>
      <FlatList
        data={items}
        keyExtractor={(i) => i.key}
        contentContainerClassName="gap-2 px-4 pb-8"
        ListEmptyComponent={
          q && results.isFetched ? (
            <EmptyState title={t("search.noResults", { query: q })} />
          ) : undefined
        }
        renderItem={({ item }) => (
          <Link href={item.href as never} asChild>
            <Pressable className="flex-row items-center gap-3 rounded-xl bg-surface p-2 active:opacity-70">
              <Poster uri={imageUrl(item.poster, "w185")} className="w-12" />
              <View className="flex-1">
                <Text numberOfLines={1} className="font-medium text-fg">
                  {item.title}
                </Text>
                {item.year ? <Text className="text-xs text-faint">{item.year}</Text> : null}
              </View>
            </Pressable>
          </Link>
        )}
      />
    </Screen>
  );
}
