import { useQuery } from "@tanstack/react-query";
import { router } from "expo-router";
import type { BottomTabBarProps } from "expo-router/tabs";
import { Search } from "lucide-react-native";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Text, View } from "react-native";
import { COLORS } from "../lib/colors";
import { meQuery } from "../lib/queries";
import { ui } from "../lib/scale";
import { Pressable } from "./Pressable";

/** The tabs a TV shows: no Downloads, nothing is stored there (D110). */
export const TV_TABS = ["home", "movies", "series", "watchlist"];

function Item({
  label,
  active,
  icon,
  onPress,
  testID,
}: {
  label: string;
  active: boolean;
  icon: ReactNode;
  onPress: () => void;
  testID?: string;
}) {
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityState={{ selected: active }}
      onPress={onPress}
      className={`items-center gap-1 rounded-xl px-1 py-2.5 ${active ? "bg-surface-2" : ""}`}
    >
      {icon}
      <Text
        numberOfLines={1}
        className={`w-full text-center text-xs font-semibold ${active ? "text-accent" : "text-muted"}`}
      >
        {label}
      </Text>
    </Pressable>
  );
}

/**
 * The TV's navigation (D112): a rail on the left, as on most TV apps, with search and the
 * account that sit at the top right of the phone's screens. OK opens a tab; the remote's
 * right arrow goes into it.
 */
export function TvSidebar({ state, descriptors, navigateToTab, emitter }: BottomTabBarProps) {
  const { t } = useTranslation();
  const { data: me } = useQuery(meQuery);
  return (
    <View className="w-28 gap-2 border-r border-line bg-surface px-3 pt-6">
      <Text className="mb-4 text-center text-base font-bold text-accent">BatVision</Text>
      {state.routes.map((route, index) => {
        if (!TV_TABS.includes(route.name)) return null;
        const options = descriptors[route.key]?.options;
        if (!options) return null;
        const active = index === state.index;
        const color = active ? COLORS.accent : COLORS.muted;
        return (
          <Item
            key={route.key}
            testID={options.tabBarButtonTestID}
            label={typeof options.title === "string" ? options.title : route.name}
            active={active}
            icon={options.tabBarIcon?.({ focused: active, color, size: ui(22) })}
            onPress={() => {
              const event = emitter.emit({
                type: "tabPress",
                target: route.key,
                canPreventDefault: true,
              });
              if (!active && !event.defaultPrevented) navigateToTab(route.key);
            }}
          />
        );
      })}
      <View className="my-2 h-px bg-line" />
      <Item
        testID="open-search"
        label={t("nav.search")}
        active={false}
        icon={<Search color={COLORS.muted} size={ui(22)} />}
        onPress={() => router.push("/search")}
      />
      <Item
        testID="open-profile"
        label={t("nav.account")}
        active={false}
        icon={
          <View className="size-6 items-center justify-center rounded-full bg-accent">
            <Text className="text-xs font-bold text-accent-ink">
              {me?.username.charAt(0).toUpperCase() ?? ""}
            </Text>
          </View>
        }
        onPress={() => router.push("/settings")}
      />
    </View>
  );
}
