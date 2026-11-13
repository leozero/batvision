import { formatBytes } from "@batvision/core/bytes";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Platform, View } from "react-native";
import type { Schemas } from "../lib/api";
import { useSaveSettings } from "../lib/queries";
import { useSyncState } from "../lib/sync";
import { type Choice, ChoiceList, SettingRow, Sheet, SwitchRow } from "./Sheet";
import { Button, Field, Input, Muted } from "./ui";

const GB = 1_000_000_000;

type SettingSheet = "profile" | "quota" | "mobile_data" | "mobile_cap" | "storage" | null;

/** Space in GB: a few common values, none ("unlimited"), or one typed in. */
function GigabytesChoice({
  value,
  presets,
  noneLabel,
  onChange,
}: {
  value: number | null | undefined;
  presets: number[];
  noneLabel: string;
  onChange: (bytes: number | null) => void;
}) {
  const { t } = useTranslation();
  const current = value ? Math.round(value / GB) : null;
  const preset = current === null || presets.includes(current);
  const [custom, setCustom] = useState(preset ? "" : String(current));
  const options: Choice<string>[] = [
    ...presets.map((n) => ({ value: String(n), label: t("mobile.gigabytes", { count: n }) })),
    { value: "none", label: noneLabel },
  ];
  return (
    <View className="gap-3">
      <ChoiceList
        value={current === null ? "none" : preset ? String(current) : ""}
        options={options}
        onChange={(v) => onChange(v === "none" ? null : Number(v) * GB)}
      />
      <Field label={t("mobile.otherValue")}>
        <View className="flex-row gap-2">
          <View className="flex-1">
            <Input
              testID="gigabytes"
              keyboardType="number-pad"
              value={custom}
              placeholder={t("mobile.gigabytesInput")}
              onChangeText={setCustom}
            />
          </View>
          <Button
            testID="gigabytes-apply"
            variant="primary"
            disabled={!Number(custom)}
            onPress={() => onChange(Number(custom) * GB)}
          >
            {t("mobile.apply")}
          </Button>
        </View>
      </Field>
    </View>
  );
}

/** This phone's settings, stored on the server so the web can change them too. */
export function DeviceSettings({ device }: { device: Schemas["DeviceDownloads"] }) {
  const { t, i18n } = useTranslation();
  const sync = useSyncState();
  const save = useSaveSettings(device.id);
  const [s, setS] = useState(device.settings);
  const [sheet, setSheet] = useState<SettingSheet>(null);
  useEffect(() => setS(device.settings), [device.settings]);
  const update = (patch: Partial<Schemas["DeviceSettings"]>) => {
    const next = { ...s, ...patch };
    setS(next);
    save.mutate(next);
    setSheet(null);
  };
  const size = (bytes: number | null | undefined, none: string) =>
    bytes ? formatBytes(bytes, i18n.language) : none;
  const profiles = (["max", "mobile", "saver"] as const).map((p) => ({
    value: p,
    label: t(`downloads.profile.${p}`),
    hint: t(`downloads.profileHint.${p}`),
  }));
  const mobileModes = (["off", "light", "all"] as const).map((m) => ({
    value: m,
    label: t(`downloads.mobileDataMode.${m}`),
    hint: t(`downloads.mobileDataHint.${m}`),
  }));
  const storages = [
    {
      value: "internal" as const,
      label: t("mobile.storagePhone"),
      hint: t("mobile.storageHint.internal"),
    },
    { value: "sd" as const, label: t("mobile.storageSd"), hint: t("mobile.storageHint.sd") },
  ];
  const close = () => setSheet(null);
  return (
    <View className="gap-3 rounded-card border border-line bg-surface p-4">
      <SwitchRow
        testID="auto-download"
        label={t("downloads.auto")}
        hint={t("downloads.autoHint")}
        value={s.auto_download}
        onChange={(v) => update({ auto_download: v })}
      />
      <SettingRow
        testID="setting-profile"
        label={t("downloads.quality")}
        value={t(`downloads.profile.${s.profile}`)}
        hint={t(`downloads.profileHint.${s.profile}`)}
        onPress={() => setSheet("profile")}
      />
      <SettingRow
        testID="setting-quota"
        label={t("mobile.quotaLabel")}
        value={size(s.quota_bytes, t("mobile.unlimited"))}
        hint={t("mobile.quotaHint")}
        onPress={() => setSheet("quota")}
      />
      <SettingRow
        testID="setting-mobile-data"
        label={t("downloads.mobileData")}
        value={t(`downloads.mobileDataMode.${s.mobile_data}`)}
        hint={t(`downloads.mobileDataHint.${s.mobile_data}`)}
        onPress={() => setSheet("mobile_data")}
      />
      {s.mobile_data !== "off" ? (
        <SettingRow
          testID="setting-mobile-cap"
          label={t("mobile.capLabel")}
          value={size(s.mobile_cap_bytes, t("mobile.noCap"))}
          hint={t("mobile.capHint")}
          onPress={() => setSheet("mobile_cap")}
        />
      ) : null}
      <SwitchRow
        label={t("downloads.chargingOnly")}
        hint={t("downloads.chargingOnlyHint")}
        value={s.charging_only}
        onChange={(v) => update({ charging_only: v })}
      />
      {/* No SD card on iOS (D89). */}
      {Platform.OS === "android" ? (
        <SettingRow
          testID="setting-storage"
          label={t("downloads.storage")}
          value={s.storage === "sd" ? t("mobile.storageSd") : t("mobile.storagePhone")}
          hint={s.storage === "sd" && !sync.sdAvailable ? t("mobile.sdMissing") : undefined}
          onPress={() => setSheet("storage")}
        />
      ) : (
        <Muted>{t("mobile.iosBackground")}</Muted>
      )}

      <Sheet open={sheet === "profile"} onClose={close} title={t("downloads.quality")}>
        <ChoiceList
          value={s.profile}
          options={profiles}
          onChange={(profile) => update({ profile })}
        />
      </Sheet>
      <Sheet
        open={sheet === "quota"}
        onClose={close}
        title={t("mobile.quotaLabel")}
        description={t("mobile.quotaHint")}
      >
        <GigabytesChoice
          value={s.quota_bytes}
          presets={[10, 20, 50, 100]}
          noneLabel={t("mobile.unlimited")}
          onChange={(quota_bytes) => update({ quota_bytes })}
        />
      </Sheet>
      <Sheet open={sheet === "mobile_data"} onClose={close} title={t("downloads.mobileData")}>
        <ChoiceList
          value={s.mobile_data}
          options={mobileModes}
          onChange={(mobile_data) => update({ mobile_data })}
        />
      </Sheet>
      <Sheet
        open={sheet === "mobile_cap"}
        onClose={close}
        title={t("mobile.capLabel")}
        description={t("mobile.capHint")}
      >
        <GigabytesChoice
          value={s.mobile_cap_bytes}
          presets={[1, 2, 5, 10]}
          noneLabel={t("mobile.noCap")}
          onChange={(mobile_cap_bytes) => update({ mobile_cap_bytes })}
        />
      </Sheet>
      <Sheet open={sheet === "storage"} onClose={close} title={t("downloads.storage")}>
        <ChoiceList
          value={s.storage}
          options={storages}
          onChange={(storage) => update({ storage })}
        />
        {!sync.sdAvailable ? <Muted>{t("mobile.sdMissing")}</Muted> : null}
      </Sheet>
    </View>
  );
}
