import { router } from "expo-router";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { KeyboardAvoidingView, ScrollView, Text, View } from "react-native";
import { Button, ErrorText, Field, Input, Muted, Screen } from "../components/ui";
import { normalizeServer } from "../lib/session";
import { BatvisionSync } from "../lib/sync";

/** First launch: where is the server? (as on desktop, D72) */
export default function Connect() {
  const { t } = useTranslation();
  const [address, setAddress] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    const origin = normalizeServer(address);
    if (!origin) return setError(t("desktop.serverErrors.invalid_url"));
    setBusy(true);
    setError(null);
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 10_000);
      const response = await fetch(`${origin}/api/health`, { signal: controller.signal });
      clearTimeout(timer);
      const body = (await response.json().catch(() => null)) as { status?: string } | null;
      if (!response.ok || body?.status !== "ok") throw new Error("not_batvision");
      BatvisionSync.setServer(origin);
      router.replace("/login");
    } catch (e) {
      setError(
        t(
          e instanceof Error && e.message === "not_batvision"
            ? "desktop.serverErrors.not_batvision"
            : "desktop.serverErrors.unreachable",
        ),
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen>
      <KeyboardAvoidingView behavior="height" className="flex-1">
        <ScrollView
          contentContainerClassName="flex-grow justify-center gap-6 p-6"
          keyboardShouldPersistTaps="handled"
        >
          <View className="gap-2">
            <Text className="text-3xl font-bold text-accent">batvision</Text>
            <Text className="text-xl font-semibold text-fg">{t("desktop.connectTitle")}</Text>
            <Muted>{t("desktop.connectIntro")}</Muted>
          </View>
          <Field label={t("desktop.serverAddress")}>
            <Input
              testID="server-address"
              value={address}
              onChangeText={setAddress}
              placeholder={t("desktop.serverPlaceholder")}
              autoCapitalize="none"
              autoCorrect={false}
              keyboardType="url"
              onSubmitEditing={submit}
            />
          </Field>
          <ErrorText>{error}</ErrorText>
          <Button testID="connect" variant="primary" loading={busy} onPress={submit}>
            {t("desktop.connect")}
          </Button>
        </ScrollView>
      </KeyboardAvoidingView>
    </Screen>
  );
}
