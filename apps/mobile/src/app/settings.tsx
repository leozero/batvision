import { languageName } from "@batvision/core/format";
import { SPOKEN_LANGUAGE_CHOICES, spokenLanguages, trackLanguage } from "@batvision/core/languages";
import { useQuery } from "@tanstack/react-query";
import { router } from "expo-router";
import { ArrowLeft, ChevronRight, Plus, ShieldCheck, X } from "lucide-react-native";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Pressable, ScrollView, Text, View } from "react-native";
import { DeviceSettings } from "../components/DeviceSettings";
import { ChoiceList, SettingRow, Sheet, SwitchRow } from "../components/Sheet";
import { Button, COLORS, EmptyState, Loading, Screen } from "../components/ui";
import { VersionBanner } from "../components/VersionBanner";
import { api, type Schemas } from "../lib/api";
import { downloadsQuery, meQuery, preferencesQuery, useSavePreferences } from "../lib/queries";
import { ui } from "../lib/scale";
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
            <ArrowLeft color={COLORS.fg} size={ui(20)} />
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
            <ShieldCheck color={COLORS.accent} size={ui(20)} />
            <Text className="flex-1 text-base font-semibold text-fg">{t("nav.admin")}</Text>
            <ChevronRight color={COLORS.faint} size={ui(20)} />
          </Pressable>
        ) : null}
        <VersionBanner className="" />
        <Tracks />
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

type SubtitleMode = Schemas["SubtitleMode"];

/** The languages someone understands and how they like the rest (D104): every device. */
function Tracks() {
  const { t, i18n } = useTranslation();
  const { data: me } = useQuery(meQuery);
  const { data: prefs } = useQuery(preferencesQuery);
  const save = useSavePreferences();
  const [modes, setModes] = useState(false);
  if (!prefs) return null;
  // A server before 0.9 knows nothing of these settings: saving would change nothing.
  if (prefs.spoken_languages == null)
    return (
      <View className="gap-3">
        <Text className="mt-2 text-lg font-semibold text-fg">{t("account.tracks")}</Text>
        <Text testID="tracks-need-server" className="text-sm text-muted">
          {t("account.needsServer")}
        </Text>
      </View>
    );
  const name = (code: string) => {
    const label = languageName(code, i18n.language) ?? code;
    return label.charAt(0).toLocaleUpperCase(i18n.language) + label.slice(1);
  };
  const spoken = spokenLanguages(prefs, trackLanguage(me?.language ?? i18n.language));
  const others = SPOKEN_LANGUAGE_CHOICES.filter((c) => !spoken.includes(c));
  const update = (patch: Partial<Schemas["PreferencesUpdate"]>) =>
    save.mutate({
      spoken_languages: spoken,
      prefer_dub: prefs.prefer_dub,
      subtitle_mode: prefs.subtitle_mode,
      ...patch,
    });
  const modeLabel: Record<SubtitleMode, string> = {
    foreign: t("account.modeForeign"),
    always: t("account.modeAlways"),
    never: t("account.modeNever"),
  };
  return (
    <View className="gap-3">
      <Text className="mt-2 text-lg font-semibold text-fg">{t("account.tracks")}</Text>
      <View className="gap-3 rounded-card border border-line bg-surface p-4">
        <View className="gap-1">
          <Text className="text-sm font-medium text-fg">{t("account.spokenLanguages")}</Text>
          <Text className="text-xs text-muted">{t("account.spokenLanguagesHint")}</Text>
        </View>
        <View className="flex-row flex-wrap gap-2">
          {spoken.map((code, i) => (
            <Pressable
              key={code}
              testID={`spoken-${code}`}
              accessibilityRole="button"
              accessibilityLabel={t("account.removeLanguage", { language: name(code) })}
              disabled={spoken.length === 1}
              onPress={() => update({ spoken_languages: spoken.filter((c) => c !== code) })}
              className="flex-row items-center gap-1.5 rounded-full border border-accent bg-accent/15 py-1.5 pl-3 pr-2 active:opacity-70"
            >
              <Text className="text-xs text-muted">{i + 1}</Text>
              <Text className="text-sm font-medium text-fg">{name(code)}</Text>
              {spoken.length > 1 ? <X color={COLORS.muted} size={ui(14)} /> : null}
            </Pressable>
          ))}
        </View>
        <View className="flex-row flex-wrap gap-2">
          {others.map((code) => (
            <Pressable
              key={code}
              testID={`add-language-${code}`}
              accessibilityRole="button"
              accessibilityLabel={t("account.addLanguage", { language: name(code) })}
              onPress={() => update({ spoken_languages: [...spoken, code] })}
              className="flex-row items-center gap-1 rounded-full border border-line bg-surface-2 py-1 pl-2 pr-3 active:opacity-70"
            >
              <Plus color={COLORS.muted} size={ui(14)} />
              <Text className="text-xs text-muted">{name(code)}</Text>
            </Pressable>
          ))}
        </View>
        <SwitchRow
          testID="prefer-dub"
          label={t("account.preferDub")}
          hint={t("account.preferDubHint")}
          value={prefs.prefer_dub}
          onChange={(prefer_dub) => update({ prefer_dub })}
        />
        <SettingRow
          testID="setting-subtitles"
          label={t("account.subtitleMode")}
          value={modeLabel[prefs.subtitle_mode]}
          onPress={() => setModes(true)}
        />
      </View>
      <Sheet open={modes} onClose={() => setModes(false)} title={t("account.subtitleMode")}>
        <ChoiceList
          value={prefs.subtitle_mode}
          options={(["foreign", "always", "never"] as const).map((value) => ({
            value,
            label: modeLabel[value],
          }))}
          onChange={(subtitle_mode) => {
            update({ subtitle_mode });
            setModes(false);
          }}
        />
      </Sheet>
    </View>
  );
}
