import { Check, ChevronRight } from "lucide-react-native";
import type { ReactNode } from "react";
import { Modal, Pressable, ScrollView, Switch, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { COLORS } from "./ui";

/** A panel that slides up from the bottom; tapping outside closes it. */
export function Sheet({
  open,
  onClose,
  title,
  description,
  children,
  testID,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children: ReactNode;
  testID?: string;
}) {
  const insets = useSafeAreaInsets();
  return (
    <Modal visible={open} transparent animationType="slide" onRequestClose={onClose}>
      <View className="flex-1 justify-end">
        <Pressable
          accessibilityLabel="close"
          className="absolute inset-0 bg-black/60"
          onPress={onClose}
        />
        <View
          testID={testID}
          className="max-h-[85%] rounded-t-3xl border-t border-line bg-surface px-4 pt-3"
          style={{ paddingBottom: insets.bottom + 16 }}
        >
          <View className="mb-3 h-1 w-10 self-center rounded-full bg-line" />
          <Text className="text-lg font-semibold text-fg">{title}</Text>
          {description ? <Text className="mt-1 text-sm text-muted">{description}</Text> : null}
          <ScrollView className="mt-4" contentContainerClassName="gap-2">
            {children}
          </ScrollView>
        </View>
      </View>
    </Modal>
  );
}

export type Choice<T extends string> = { value: T; label: string; hint?: string };

/** Radio options, each with a line saying what it does. */
export function ChoiceList<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: Choice<T>[];
  onChange: (value: T) => void;
}) {
  return (
    <View className="gap-2" accessibilityRole="radiogroup">
      {options.map((o) => {
        const selected = o.value === value;
        return (
          <Pressable
            key={o.value}
            testID={`choice-${o.value}`}
            accessibilityRole="radio"
            accessibilityState={{ checked: selected }}
            onPress={() => onChange(o.value)}
            className={`flex-row items-center gap-3 rounded-xl border px-4 py-3 active:opacity-70 ${
              selected ? "border-accent bg-accent/10" : "border-line bg-surface-2"
            }`}
          >
            <View
              className={`size-5 items-center justify-center rounded-full border-2 ${
                selected ? "border-accent bg-accent" : "border-faint"
              }`}
            >
              {selected ? <Check color={COLORS.accentInk} size={12} strokeWidth={3} /> : null}
            </View>
            <View className="flex-1 gap-0.5">
              <Text className="text-sm font-semibold text-fg">{o.label}</Text>
              {o.hint ? <Text className="text-xs text-muted">{o.hint}</Text> : null}
            </View>
          </Pressable>
        );
      })}
    </View>
  );
}

/** A setting: its name, the current value, and a sheet to change it. */
export function SettingRow({
  label,
  value,
  hint,
  onPress,
  testID,
}: {
  label: string;
  value: string;
  hint?: string;
  onPress: () => void;
  testID?: string;
}) {
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      onPress={onPress}
      className="flex-row items-center gap-3 rounded-xl bg-surface-2 px-4 py-3 active:opacity-70"
    >
      <View className="flex-1 gap-0.5">
        <Text className="text-sm font-medium text-fg">{label}</Text>
        <Text className="text-sm text-accent">{value}</Text>
        {hint ? <Text className="text-xs text-muted">{hint}</Text> : null}
      </View>
      <ChevronRight color={COLORS.faint} size={18} />
    </Pressable>
  );
}

/** An on/off setting with a line explaining it. */
export function SwitchRow({
  label,
  hint,
  value,
  onChange,
  testID,
}: {
  label: string;
  hint?: string;
  value: boolean;
  onChange: (value: boolean) => void;
  testID?: string;
}) {
  return (
    <View className="flex-row items-center gap-4 rounded-xl bg-surface-2 px-4 py-3">
      <View className="flex-1 gap-0.5">
        <Text className="text-sm font-medium text-fg">{label}</Text>
        {hint ? <Text className="text-xs text-muted">{hint}</Text> : null}
      </View>
      <Switch
        testID={testID}
        value={value}
        onValueChange={onChange}
        trackColor={{ true: COLORS.accent }}
      />
    </View>
  );
}
