import { useQuery } from "@tanstack/react-query";
import { router } from "expo-router";
import { ArrowLeft } from "lucide-react-native";
import { useTranslation } from "react-i18next";
import { Pressable, ScrollView, Text, View } from "react-native";
import { DeviceSettings } from "../components/DeviceSettings";
import { Button, COLORS, EmptyState, Loading, Screen } from "../components/ui";
import { api } from "../lib/api";
import { downloadsQuery } from "../lib/queries";
import { session } from "../lib/session";
import { BatvisionSync } from "../lib/sync";

/** This device's download settings and sign-out, out of the Downloads tab's way. */
export default function Settings() {
  const { t } = useTranslation();
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
            onPress={() => (router.canGoBack() ? router.back() : router.replace("/downloads"))}
            className="size-10 items-center justify-center rounded-full bg-surface-2 active:opacity-70"
          >
            <ArrowLeft color={COLORS.fg} size={20} />
          </Pressable>
          <Text className="flex-1 text-2xl font-bold text-fg">{t("mobile.settings")}</Text>
        </View>
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
