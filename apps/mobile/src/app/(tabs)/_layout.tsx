import { useQuery } from "@tanstack/react-query";
import { Redirect, Tabs } from "expo-router";
import { Download, Film, Home, Search, Tv } from "lucide-react-native";
import { useTranslation } from "react-i18next";
import { COLORS, Loading } from "../../components/ui";
import { Unreachable } from "../../lib/api";
import { meQuery } from "../../lib/queries";
import { BatvisionSync } from "../../lib/sync";

export default function TabsLayout() {
  const { t } = useTranslation();
  const me = useQuery(meQuery);

  if (me.error instanceof Unreachable) return <Redirect href="/offline" />;
  if (me.isPending) return <Loading />;
  if (me.data === null) {
    // The token was revoked or expired.
    BatvisionSync.signOut();
    return <Redirect href="/login" />;
  }

  return (
    <Tabs
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: COLORS.accent,
        tabBarInactiveTintColor: COLORS.faint,
        tabBarStyle: { backgroundColor: COLORS.surface, borderTopColor: COLORS.line },
        sceneStyle: { backgroundColor: COLORS.bg },
      }}
    >
      <Tabs.Screen
        name="home"
        options={{
          tabBarButtonTestID: "tab-home",
          title: t("nav.home"),
          tabBarIcon: ({ color }) => <Home color={color} size={22} />,
        }}
      />
      <Tabs.Screen
        name="movies"
        options={{
          tabBarButtonTestID: "tab-movies",
          title: t("nav.movies"),
          tabBarIcon: ({ color }) => <Film color={color} size={22} />,
        }}
      />
      <Tabs.Screen
        name="series"
        options={{
          tabBarButtonTestID: "tab-series",
          title: t("nav.series"),
          tabBarIcon: ({ color }) => <Tv color={color} size={22} />,
        }}
      />
      <Tabs.Screen
        name="search"
        options={{
          tabBarButtonTestID: "tab-search",
          title: t("nav.search"),
          tabBarIcon: ({ color }) => <Search color={color} size={22} />,
        }}
      />
      <Tabs.Screen
        name="downloads"
        options={{
          tabBarButtonTestID: "tab-downloads",
          title: t("nav.downloads"),
          tabBarIcon: ({ color }) => <Download color={color} size={22} />,
        }}
      />
    </Tabs>
  );
}
