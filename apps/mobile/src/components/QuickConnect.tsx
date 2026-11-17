import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ActivityIndicator, Text, View } from "react-native";
import { ApiError, api, call, type Schemas, Unreachable } from "../lib/api";
import { COLORS } from "../lib/colors";
import { approveUrl, spacedCode } from "../lib/quickConnect";
import { ui } from "../lib/scale";
import { deviceName, devicePlatform, session } from "../lib/session";
import { IS_TV } from "../lib/tv";
import { QrCode } from "./QrCode";
import { ErrorText, Muted } from "./ui";

/** How often the waiting device asks whether its code was approved. */
const POLL_MS = 3_000;

type Started = Schemas["QuickConnectStarted"];

/**
 * Quick connect (spec 6, D111): this device shows a code, a signed-in phone or computer
 * approves it, and this device receives its own token. A new code replaces an expired one.
 */
export function QuickConnect({
  onSignedIn,
}: {
  onSignedIn: (token: string | null | undefined, deviceId: number) => void;
}) {
  const { t } = useTranslation();
  const [started, setStarted] = useState<Started | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Bumped when a code expires: the next one is asked for.
  const [round, setRound] = useState(0);
  // The latest callback, without restarting the polling when the parent renders again.
  const signedIn = useRef(onSignedIn);
  signedIn.current = onSignedIn;

  // biome-ignore lint/correctness/useExhaustiveDependencies: a new code each round
  useEffect(() => {
    let current = true;
    setStarted(null);
    call(
      api().POST("/api/auth/quick-connect/start", {
        body: { device: { name: deviceName(), platform: devicePlatform() } },
      }),
    )
      .then((s) => {
        if (!current) return;
        setError(null);
        setStarted(s);
      })
      .catch((e) => {
        if (!current) return;
        if (e instanceof Unreachable) setError(t("errors.network"));
        else if (e instanceof ApiError)
          setError(t(`errors.${e.code}`, { defaultValue: t("tv.codeError"), seconds: 60 }));
        else setError(t("tv.codeError"));
        // Try again a little later: the server may come back, the limit may lift.
        setTimeout(() => current && setRound((r) => r + 1), 15_000);
      });
    return () => {
      current = false;
    };
  }, [round]);

  useEffect(() => {
    if (!started) return;
    let current = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const result = await call(
          api().POST("/api/auth/quick-connect/poll", {
            body: { poll_secret: started.poll_secret },
          }),
        );
        if (!current) return;
        if (result.status === "ok") return signedIn.current(result.token, result.device_id);
        if (result.status === "expired") return setRound((r) => r + 1);
      } catch {
        // A blip on the network: the next poll tells.
      }
      if (current) timer = setTimeout(poll, POLL_MS);
    };
    timer = setTimeout(poll, POLL_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [started]);

  const server = session().server;
  return (
    <View className={IS_TV ? "flex-row items-center gap-10" : "gap-5"}>
      <View className="flex-1 gap-4">
        <Text className="text-xl font-semibold text-fg">{t("tv.codeTitle")}</Text>
        <Muted>{t("tv.codeIntro")}</Muted>
        <View className="min-h-20 justify-center">
          {started ? (
            <Text
              testID="quick-connect-code"
              accessibilityLabel={started.code.split("").join(" ")}
              className="text-5xl font-bold tracking-widest text-accent"
            >
              {spacedCode(started.code)}
            </Text>
          ) : error ? null : (
            <ActivityIndicator color={COLORS.accent} size="large" className="self-start" />
          )}
        </View>
        <ErrorText>{error}</ErrorText>
        {started ? (
          <View className="flex-row items-center gap-2">
            <ActivityIndicator color={COLORS.muted} size="small" />
            <Muted>
              {t("tv.waiting")} {t("tv.renews")}
            </Muted>
          </View>
        ) : null}
      </View>
      {/* A phone scans the TV's screen; a phone's own screen can't be scanned by itself. */}
      {IS_TV && started && server ? (
        <View className="w-56 items-center gap-3">
          <View className="overflow-hidden rounded-xl">
            <QrCode
              testID="quick-connect-qr"
              value={approveUrl(server, started.code)}
              size={ui(200)}
            />
          </View>
          <Text className="text-center text-xs text-muted">{t("tv.scan")}</Text>
        </View>
      ) : null}
    </View>
  );
}
