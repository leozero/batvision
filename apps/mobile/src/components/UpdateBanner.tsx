import { isNewer } from "@batvision/core/version";
import { useQuery } from "@tanstack/react-query";
import Constants from "expo-constants";
import { Download } from "lucide-react-native";
import { useTranslation } from "react-i18next";
import { Linking, Pressable, Text } from "react-native";
import { COLORS } from "./ui";

const LATEST_RELEASE = "https://api.github.com/repos/leozero/batvision/releases/latest";

type Release = { tag_name: string; assets: { name: string; browser_download_url: string }[] };

/** A newer APK on the GitHub releases (D66): tapping downloads it, Android offers to install. */
export function UpdateBanner() {
  const { t } = useTranslation();
  const release = useQuery({
    queryKey: ["latest-release"],
    queryFn: async (): Promise<Release | null> => {
      const response = await fetch(LATEST_RELEASE, {
        headers: { accept: "application/vnd.github+json" },
      });
      return response.ok ? ((await response.json()) as Release) : null;
    },
    staleTime: 6 * 3600_000,
    retry: false,
  });
  const current = Constants.expoConfig?.version ?? "0.0.0";
  const apk = release.data?.assets.find((a) => a.name.endsWith(".apk"));
  if (!release.data || !apk || !isNewer(release.data.tag_name, current)) return null;
  return (
    <Pressable
      onPress={() => void Linking.openURL(apk.browser_download_url)}
      className="mx-4 flex-row items-center gap-2 rounded-xl bg-accent/15 px-4 py-3"
    >
      <Download color={COLORS.accent} size={16} />
      <Text className="flex-1 text-sm text-accent">
        {t("mobile.updateAvailable", { version: release.data.tag_name.replace(/^v/, "") })}
      </Text>
    </Pressable>
  );
}
