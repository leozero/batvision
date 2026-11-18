import { useMutation } from "@tanstack/react-query";
import { router, useLocalSearchParams } from "expo-router";
import { ArrowLeft, CheckCircle2 } from "lucide-react-native";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { KeyboardAvoidingView, ScrollView, Text, View } from "react-native";
import { Pressable } from "../components/Pressable";
import { Button, COLORS, ErrorText, Input, Muted, Screen } from "../components/ui";
import { ApiError, api, call, Unreachable } from "../lib/api";
import { typedCode } from "../lib/quickConnect";
import { ui } from "../lib/scale";

/** Approves the code a TV shows (spec 6, D111): it signs in as this account. */
export default function ConnectTv() {
  const { t } = useTranslation();
  const params = useLocalSearchParams<{ code?: string }>();
  const [code, setCode] = useState((params.code ?? "").toUpperCase());
  const [done, setDone] = useState(false);
  const approve = useMutation({
    mutationFn: (value: string) =>
      call(api().POST("/api/auth/quick-connect/approve", { body: { code: value } })),
    onSuccess: () => {
      setDone(true);
      setCode("");
    },
  });
  const error = approve.error;
  const message =
    error instanceof Unreachable
      ? t("errors.network")
      : error instanceof ApiError
        ? error.status === 404
          ? t("tv.codeUnknown")
          : t(`errors.${error.code}`, { defaultValue: error.message, seconds: 60 })
        : error
          ? String(error)
          : null;
  const submit = () => {
    const value = typedCode(code);
    if (!value) return;
    setDone(false);
    approve.mutate(value);
  };
  return (
    <Screen>
      <KeyboardAvoidingView behavior="padding" className="flex-1">
        <ScrollView
          contentContainerClassName="w-full max-w-lg gap-5 self-center p-4"
          keyboardShouldPersistTaps="handled"
        >
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
            <Text className="flex-1 text-2xl font-bold text-fg">{t("account.quickConnect")}</Text>
          </View>
          <Muted>{t("tv.connectHint")}</Muted>
          <Input
            testID="tv-code"
            value={code}
            onChangeText={(v) => setCode(v.toUpperCase())}
            maxLength={7}
            autoCapitalize="characters"
            autoCorrect={false}
            autoFocus
            placeholder="ABC 123"
            onSubmitEditing={submit}
            className="rounded-xl border border-line bg-surface px-4 py-3 text-center text-2xl font-bold tracking-widest text-fg"
          />
          <ErrorText>{message}</ErrorText>
          {done ? (
            <View testID="tv-connected" className="flex-row items-center gap-2">
              <CheckCircle2 color={COLORS.success} size={ui(18)} />
              <Text className="text-sm font-semibold text-success">
                {t("account.deviceConnected")}
              </Text>
            </View>
          ) : null}
          <Button
            testID="approve-tv"
            variant="primary"
            loading={approve.isPending}
            disabled={!typedCode(code)}
            onPress={submit}
          >
            {t("tv.approve")}
          </Button>
        </ScrollView>
      </KeyboardAvoidingView>
    </Screen>
  );
}
