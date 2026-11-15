import { versionGap } from "@batvision/core/version";
import { useQuery } from "@tanstack/react-query";
import Constants from "expo-constants";
import { TriangleAlert } from "lucide-react-native";
import { useTranslation } from "react-i18next";
import { Platform, Text, View } from "react-native";
import { api, call } from "../lib/api";
import { ui } from "../lib/scale";
import { COLORS } from "./ui";

/**
 * The app and the server aren't the same version (D107): a server behind ignores what's
 * new here (settings that don't stick), an app behind can't show what's new there.
 */
export function VersionBanner({ className = "mx-4" }: { className?: string }) {
  const { t } = useTranslation();
  const health = useQuery({
    queryKey: ["health"],
    queryFn: () => call(api().GET("/api/health")),
    staleTime: 10 * 60_000,
    retry: false,
  });
  const app = Constants.expoConfig?.version ?? "0.0.0";
  const server = health.data?.version;
  const gap = server ? versionGap(app, server) : null;
  // Android: the update banner already offers the newer app.
  if (!gap || (gap === "app-older" && Platform.OS === "android")) return null;
  return (
    <View
      testID="version-banner"
      className={`${className} flex-row items-center gap-2 rounded-xl bg-danger/15 px-4 py-3`}
    >
      <TriangleAlert color={COLORS.danger} size={ui(16)} />
      <Text className="flex-1 text-sm text-danger">
        {t(gap === "server-older" ? "common.serverOlder" : "common.appOlder", { app, server })}
      </Text>
    </View>
  );
}
