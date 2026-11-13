import { Image } from "expo-image";
import { router } from "expo-router";
import { ArrowLeft, CheckCircle2, Download, HardDriveDownload } from "lucide-react-native";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Pressable, Text, useWindowDimensions, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useLocalItem } from "../lib/sync";
import { COLORS } from "./ui";

/** Backdrop with a back button, title block below. */
export function Hero({ backdrop, children }: { backdrop?: string; children: ReactNode }) {
  const { t } = useTranslation();
  const insets = useSafeAreaInsets();
  // A 16:9 picture as wide as a landscape tablet would fill the screen (D85).
  const { width, height } = useWindowDimensions();
  return (
    <View>
      <View
        style={{ height: Math.min((width * 9) / 16, height * 0.55) }}
        className="w-full bg-surface-2"
      >
        {backdrop ? (
          <Image
            source={{ uri: backdrop }}
            style={{ width: "100%", height: "100%" }}
            contentFit="cover"
          />
        ) : null}
        <View className="absolute inset-x-0 bottom-0 h-24 bg-bg/60" />
      </View>
      <Pressable
        testID="back"
        accessibilityRole="button"
        accessibilityLabel={t("common.back")}
        onPress={() => (router.canGoBack() ? router.back() : router.replace("/home"))}
        style={{ top: insets.top + 8 }}
        className="absolute left-3 size-10 items-center justify-center rounded-full bg-black/50"
      >
        <ArrowLeft color={COLORS.fg} size={20} />
      </Pressable>
      <View className="-mt-10 w-full max-w-3xl gap-2 self-center px-4">{children}</View>
    </View>
  );
}

/** "Downloaded" / download progress for this phone's copy. */
export function LocalMark({ kind, id }: { kind: "movie" | "episode"; id: number }) {
  const { t } = useTranslation();
  const item = useLocalItem(kind, id);
  if (!item) return null;
  if (item.status === "done")
    return (
      <View testID="local-done" className="flex-row items-center gap-1">
        <HardDriveDownload color={COLORS.accent} size={14} />
        <Text className="text-xs font-semibold text-accent">{t("desktop.downloaded")}</Text>
      </View>
    );
  return (
    <View testID="local-waiting" className="flex-row items-center gap-1">
      <Download color={COLORS.faint} size={14} />
      <Text className="text-xs text-faint">
        {item.wait ? t(`mobile.wait.${item.wait}`) : t("desktop.state.waiting")}
      </Text>
    </View>
  );
}

export function WatchedMark() {
  const { t } = useTranslation();
  return (
    <View className="flex-row items-center gap-1">
      <CheckCircle2 color={COLORS.success} size={14} />
      <Text className="text-xs font-semibold text-success">{t("detail.watched")}</Text>
    </View>
  );
}
