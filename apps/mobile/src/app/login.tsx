import { useQueryClient } from "@tanstack/react-query";
import { router } from "expo-router";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { KeyboardAvoidingView, ScrollView, Text, View } from "react-native";
import { Button, ErrorText, Field, Input, Muted, Screen } from "../components/ui";
import { ApiError, api, call, Unreachable } from "../lib/api";
import { deviceName, session } from "../lib/session";
import { BatvisionSync } from "../lib/sync";

/** Password, then the 2FA code when the account has one (D72). */
export default function Login() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [challenge, setChallenge] = useState<string | null>(null);
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const signedIn = (token: string | null | undefined, deviceId: number) => {
    if (!token) throw new Error("no token");
    BatvisionSync.signIn(token, deviceId);
    qc.clear();
    router.replace("/home");
  };

  const fail = (e: unknown) => {
    if (e instanceof Unreachable) setError(t("errors.network"));
    else if (e instanceof ApiError)
      setError(t(`errors.${e.code}`, { defaultValue: e.message, seconds: 60 }));
    else setError(String(e));
  };

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      if (challenge) {
        const s = await call(
          api().POST("/api/auth/login/totp", { body: { challenge, code: code.trim() } }),
        );
        signedIn(s.token, s.device_id);
        return;
      }
      const result = await call(
        api().POST("/api/auth/login", {
          body: {
            username: username.trim(),
            password,
            device: { name: deviceName(), platform: "android" },
          },
        }),
      );
      if (result.status === "totp_required") setChallenge(result.challenge);
      else signedIn(result.token, result.device_id);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen>
      <KeyboardAvoidingView behavior="padding" className="flex-1">
        <ScrollView
          contentContainerClassName="flex-grow justify-center gap-5 p-6"
          keyboardShouldPersistTaps="handled"
        >
          <View className="gap-2">
            <Text className="text-3xl font-bold text-accent">BatVision</Text>
            <Text className="text-xl font-semibold text-fg">{t("auth.welcome")}</Text>
            <Muted>{challenge ? t("auth.totpPrompt") : t("auth.signInPrompt")}</Muted>
            <Muted className="text-faint">{session().server}</Muted>
          </View>
          {challenge ? (
            <Field label={t("auth.code")}>
              <Input
                testID="totp-code"
                value={code}
                onChangeText={setCode}
                keyboardType="number-pad"
                autoFocus
                onSubmitEditing={submit}
              />
            </Field>
          ) : (
            <>
              <Field label={t("auth.username")}>
                <Input
                  testID="username"
                  value={username}
                  onChangeText={setUsername}
                  autoCapitalize="none"
                  autoCorrect={false}
                  textContentType="username"
                />
              </Field>
              <Field label={t("auth.password")}>
                <Input
                  testID="password"
                  value={password}
                  onChangeText={setPassword}
                  secureTextEntry
                  textContentType="password"
                  onSubmitEditing={submit}
                />
              </Field>
            </>
          )}
          <ErrorText>{error}</ErrorText>
          <Button testID="sign-in" variant="primary" loading={busy} onPress={submit}>
            {challenge ? t("auth.verify") : t("auth.signIn")}
          </Button>
          <Button
            onPress={() => {
              BatvisionSync.setServer(null);
              router.replace("/connect");
            }}
          >
            {t("desktop.changeServer")}
          </Button>
        </ScrollView>
      </KeyboardAvoidingView>
    </Screen>
  );
}
