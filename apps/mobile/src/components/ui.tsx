import { Image } from "expo-image";
import { Film } from "lucide-react-native";
import type { ComponentProps, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  ActivityIndicator,
  Pressable,
  Text,
  TextInput,
  type TextInputProps,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import type { Schemas } from "../lib/api";

export const COLORS = {
  bg: "#09090c",
  surface: "#131318",
  surface2: "#1c1c23",
  line: "#2c2c36",
  fg: "#f4f4f6",
  muted: "#a2a2b0",
  faint: "#6b6b78",
  accent: "#f4b740",
  accentInk: "#1a1405",
  danger: "#f0616d",
  success: "#4ccf8f",
};

export function Screen({
  children,
  edges,
}: {
  children: ReactNode;
  edges?: ComponentProps<typeof SafeAreaView>["edges"];
}) {
  return (
    <SafeAreaView className="flex-1 bg-bg" edges={edges ?? ["top"]}>
      {children}
    </SafeAreaView>
  );
}

export function Title({ children }: { children: ReactNode }) {
  return <Text className="text-2xl font-bold text-fg">{children}</Text>;
}

export function Muted({ children, className }: { children: ReactNode; className?: string }) {
  return <Text className={`text-sm text-muted ${className ?? ""}`}>{children}</Text>;
}

type ButtonProps = {
  children: ReactNode;
  onPress?: () => void;
  variant?: "primary" | "ghost" | "danger";
  loading?: boolean;
  disabled?: boolean;
  icon?: ReactNode;
  testID?: string;
};

export function Button({
  children,
  onPress,
  variant = "ghost",
  loading,
  disabled,
  icon,
  testID,
}: ButtonProps) {
  const tone =
    variant === "primary" ? "bg-accent" : variant === "danger" ? "bg-danger/15" : "bg-surface-2";
  const text =
    variant === "primary" ? "text-accent-ink" : variant === "danger" ? "text-danger" : "text-fg";
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      onPress={onPress}
      disabled={disabled || loading}
      className={`flex-row items-center justify-center gap-2 rounded-xl px-4 py-3 active:opacity-70 ${tone} ${disabled ? "opacity-50" : ""}`}
    >
      {loading ? (
        <ActivityIndicator color={variant === "primary" ? COLORS.accentInk : COLORS.fg} />
      ) : (
        icon
      )}
      {typeof children === "string" ? (
        <Text className={`font-semibold ${text}`}>{children}</Text>
      ) : (
        children
      )}
    </Pressable>
  );
}

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <View className="gap-1.5">
      <Text className="text-sm font-medium text-muted">{label}</Text>
      {children}
    </View>
  );
}

export function Input(props: TextInputProps) {
  return (
    <TextInput
      placeholderTextColor={COLORS.faint}
      className="rounded-xl border border-line bg-surface px-4 py-3 text-base text-fg"
      {...props}
    />
  );
}

export function ErrorText({ children }: { children: ReactNode }) {
  if (!children) return null;
  return <Text className="text-sm text-danger">{children}</Text>;
}

export function ProgressBar({ value, className }: { value: number; className?: string }) {
  return (
    <View className={`h-1.5 overflow-hidden rounded-full bg-surface-2 ${className ?? ""}`}>
      <View
        className="h-full rounded-full bg-accent"
        style={{ width: `${Math.round(Math.min(1, Math.max(0, value)) * 100)}%` }}
      />
    </View>
  );
}

export function Badge({
  children,
  tone = "neutral",
}: {
  children: ReactNode;
  tone?: "neutral" | "accent" | "success";
}) {
  const bg =
    tone === "accent" ? "bg-accent/15" : tone === "success" ? "bg-success/15" : "bg-surface-2";
  const fg = tone === "accent" ? "text-accent" : tone === "success" ? "text-success" : "text-muted";
  return (
    <View className={`flex-row items-center gap-1 self-start rounded-full px-2.5 py-1 ${bg}`}>
      {typeof children === "string" ? (
        <Text className={`text-xs font-semibold ${fg}`}>{children}</Text>
      ) : (
        children
      )}
    </View>
  );
}

/** Poster with a fallback icon; `progress` draws the resume bar. */
export function Poster({
  uri,
  progress,
  className,
}: {
  uri?: string | null;
  progress?: number;
  className?: string;
}) {
  return (
    <View className={`aspect-[2/3] overflow-hidden rounded-card bg-surface-2 ${className ?? ""}`}>
      {uri ? (
        <Image
          source={{ uri }}
          style={{ width: "100%", height: "100%" }}
          contentFit="cover"
          transition={200}
        />
      ) : (
        <View className="flex-1 items-center justify-center">
          <Film color={COLORS.faint} size={28} />
        </View>
      )}
      {progress ? (
        <View className="absolute inset-x-2 bottom-2">
          <ProgressBar value={progress} />
        </View>
      ) : null}
    </View>
  );
}

export function Loading() {
  return (
    <View className="flex-1 items-center justify-center bg-bg">
      <ActivityIndicator color={COLORS.accent} size="large" />
    </View>
  );
}

export function EmptyState({
  icon,
  title,
  children,
}: {
  icon?: ReactNode;
  title: string;
  children?: ReactNode;
}) {
  return (
    <View className="items-center gap-3 px-8 py-16">
      {icon}
      <Text className="text-center text-base font-semibold text-fg">{title}</Text>
      {children}
    </View>
  );
}

/** "★ 7,8": green from 7/10, red under 5/10, like the web (D59). */
export function Rating({ ratings }: { ratings: Schemas["Rating"][] | null | undefined }) {
  const { i18n } = useTranslation();
  const rating = ratings?.find((r) => r.source === "tmdb") ?? ratings?.[0];
  if (!rating) return null;
  const outOf10 = (rating.value / rating.scale) * 10;
  const tone = outOf10 >= 7 ? "text-success" : outOf10 < 5 ? "text-danger" : "text-fg";
  const value = rating.value.toLocaleString(i18n.language, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  });
  return <Text className={`text-sm font-semibold ${tone}`}>★ {value}</Text>;
}
