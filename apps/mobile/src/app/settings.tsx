import {
  LANGUAGE_PRESETS,
  presetOf,
  presetPreferences,
  trackLanguage,
} from "@batvision/core/languages";
import { useQuery } from "@tanstack/react-query";
import { router } from "expo-router";
import { ArrowLeft, ChevronRight, ShieldCheck } from "lucide-react-native";
import { useTranslation } from "react-i18next";
import { Pressable, ScrollView, Text, View } from "react-native";
import { DeviceSettings } from "../components/DeviceSettings";
import { Button, COLORS, EmptyState, Loading, Screen } from "../components/ui";
import { api } from "../lib/api";
import { downloadsQuery, meQuery, preferencesQuery, useSavePreferences } from "../lib/queries";
import { session } from "../lib/session";
import { BatvisionSync } from "../lib/sync";

/** The account (with the administration, for admins), this device's download settings
 * and sign-out. Opened from the profile button and the Downloads tab. */
export default function Settings() {
  const { t } = useTranslation();
  const { data: me } = useQuery(meQuery);
  const overview = useQuery(downloadsQuery);
  const deviceId = session().deviceId;
  const device = overview.data?.devices.find((d) => d.id === deviceId);
  const signOut = async () => {
    await api()
      .POST("/api/auth/logout")
      .catch(() => {});
    BatvisionSync.signOut();
    router.replace("/login");
  };
  return (
    <Screen>
      <ScrollView contentContainerClassName="w-full max-w-3xl gap-5 self-center p-4 pb-10">
        <View className="flex-row items-center gap-3">
          <Pressable
            testID="back"
            accessibilityRole="button"
            accessibilityLabel={t("common.back")}
            hitSlop={8}
            onPress={() => (router.canGoBack() ? router.back() : router.replace("/home"))}
            className="size-10 items-center justify-center rounded-full bg-surface-2 active:opacity-70"
          >
            <ArrowLeft color={COLORS.fg} size={20} />
          </Pressable>
          <Text className="flex-1 text-2xl font-bold text-fg">{t("nav.account")}</Text>
        </View>
        {me ? (
          <View className="flex-row items-center gap-3 rounded-card border border-line bg-surface p-4">
            <View className="size-12 items-center justify-center rounded-full bg-accent">
              <Text className="text-lg font-bold text-accent-ink">
                {me.username.charAt(0).toUpperCase()}
              </Text>
            </View>
            <View className="flex-1">
              <Text className="text-base font-semibold text-fg">{me.username}</Text>
              <Text className="text-sm text-muted">
                {t(me.role === "admin" ? "account.roleAdmin" : "account.roleUser")}
              </Text>
            </View>
          </View>
        ) : null}
        {me?.role === "admin" ? (
          <Pressable
            testID="open-admin"
            accessibilityRole="button"
            onPress={() => router.push("/admin")}
            className="flex-row items-center gap-3 rounded-card border border-line bg-surface p-4 active:opacity-70"
          >
            <ShieldCheck color={COLORS.accent} size={20} />
            <Text className="flex-1 text-base font-semibold text-fg">{t("nav.admin")}</Text>
            <ChevronRight color={COLORS.faint} size={20} />
          </Pressable>
        ) : null}
        <LanguagePreset />
        <Text className="mt-2 text-lg font-semibold text-fg">{t("mobile.settings")}</Text>
        {device ? (
          <DeviceSettings device={device} />
        ) : overview.isPending ? (
          <Loading />
        ) : (
          // The overview failed, or doesn't list this device: sign-out stays within reach.
          <EmptyState title={t("common.loadError")}>
            <Button
              testID="settings-retry"
              variant="primary"
              loading={overview.isFetching}
              onPress={() => void overview.refetch()}
            >
              {t("common.retry")}
            </Button>
          </EmptyState>
        )}
        <Button testID="sign-out" variant="danger" onPress={signOut}>
          {t("mobile.signOut")}
        </Button>
      </ScrollView>
    </Screen>
  );
}

/** Dubbed, original with subtitles, or original alone (D103): for every device. */
function LanguagePreset() {
  const { t, i18n } = useTranslation();
  const { data: me } = useQuery(meQuery);
  const { data: prefs } = useQuery(preferencesQuery);
  const save = useSavePreferences();
  if (!prefs) return null;
  const language = trackLanguage(me?.language ?? i18n.language);
  const current = presetOf(prefs, language);
  return (
    <View className="gap-2">
      <Text className="mt-2 text-lg font-semibold text-fg">{t("account.howYouWatch")}</Text>
      {LANGUAGE_PRESETS.map((p) => (
        <Pressable
          key={p}
          testID={`preset-${p}`}
          accessibilityRole="button"
          accessibilityState={{ selected: current === p }}
          onPress={() => save.mutate({ ...prefs, ...presetPreferences(p, language) })}
          className={`rounded-card border p-4 active:opacity-70 ${
            current === p ? "border-accent bg-accent/10" : "border-line bg-surface"
          }`}
        >
          <Text className="text-base font-semibold text-fg">{t(`account.preset.${p}`)}</Text>
          <Text className="mt-1 text-sm text-muted">{t(`account.preset.${p}Hint`)}</Text>
        </Pressable>
      ))}
      {current === "custom" ? (
        <Text className="text-xs text-faint">{t("mobile.customTracks")}</Text>
      ) : null}
    </View>
  );
}
