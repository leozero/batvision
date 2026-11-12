import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import type { TFunction } from "i18next";
import { Copy, Laptop, MonitorSmartphone, ShieldCheck, Smartphone, Trash2, Tv } from "lucide-react";
import { motion } from "motion/react";
import QRCode from "qrcode";
import { type FormEvent, type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Badge,
  Button,
  ErrorText,
  Field,
  Input,
  Page,
  Select,
  TabPanel,
  Tabs,
} from "../../components/ui";
import { METADATA_LANGUAGE, type UiLanguage, uiLanguageFor } from "../../i18n";
import { ApiError, api, call, type Schemas } from "../../lib/api";
import { devicesQuery, meQuery, preferencesQuery } from "../../lib/queries";

export function Card({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <motion.section
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      className="rounded-2xl border border-line bg-surface p-6"
    >
      <h2 className="text-lg font-semibold">{title}</h2>
      {description ? <p className="mt-1 text-sm text-muted">{description}</p> : null}
      <div className="mt-5">{children}</div>
    </motion.section>
  );
}

export function errorText(t: TFunction, e: unknown) {
  if (e instanceof ApiError)
    return t(`errors.${e.code}`, { defaultValue: e.message, seconds: e.retryAfter ?? 30 });
  return t("errors.network");
}

export function AccountPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { tab = "profile" } = useSearch({ strict: false }) as { tab?: string };
  const tabs = [
    { value: "profile", label: t("account.profile") },
    { value: "playback", label: t("account.playback") },
    { value: "security", label: t("account.security") },
    { value: "devices", label: t("account.devices") },
  ];
  return (
    <Page className="mx-auto max-w-3xl px-4 pt-8 sm:px-6">
      <h1 className="mb-6 text-3xl font-bold tracking-tight">{t("nav.account")}</h1>
      <Tabs
        value={tab}
        onValueChange={(v) => navigate({ to: "/account", search: { tab: v }, replace: true })}
        tabs={tabs}
      >
        <TabPanel value="profile" className="space-y-6">
          <ProfileCard />
        </TabPanel>
        <TabPanel value="playback" className="space-y-6">
          <PlaybackCard />
        </TabPanel>
        <TabPanel value="security" className="space-y-6">
          <TwoFactorCard />
          <PasswordCard />
        </TabPanel>
        <TabPanel value="devices" className="space-y-6">
          <QuickConnectCard />
          <DevicesCard />
        </TabPanel>
      </Tabs>
    </Page>
  );
}

function ProfileCard() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const { data: me } = useQuery(meQuery);
  const update = useMutation({
    mutationFn: (language: string) => call(api.PATCH("/api/me", { body: { language } })),
    onSuccess: async (user) => {
      qc.setQueryData(meQuery.queryKey, user);
      await i18n.changeLanguage(uiLanguageFor(user.language));
      void qc.invalidateQueries();
    },
  });
  return (
    <Card title={t("account.language")} description={t("account.languageHint")}>
      <Select
        value={uiLanguageFor(me?.language)}
        onChange={(e) => update.mutate(METADATA_LANGUAGE[e.target.value as UiLanguage])}
        disabled={update.isPending}
      >
        <option value="fr">Français</option>
        <option value="en">English</option>
      </Select>
    </Card>
  );
}

const TRACK_LANGUAGES = ["fre", "eng", "ger", "spa", "ita", "por", "jpn", "kor"];

function PlaybackCard() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const { data: prefs } = useQuery(preferencesQuery);
  const save = useMutation({
    mutationFn: (body: Schemas["Preferences"]) => call(api.PUT("/api/me/preferences", { body })),
    onSuccess: (p) => qc.setQueryData(preferencesQuery.queryKey, p),
  });
  if (!prefs) return null;
  const names = new Intl.DisplayNames([i18n.language], { type: "language" });
  const short: Record<string, string> = {
    fre: "fr",
    eng: "en",
    ger: "de",
    spa: "es",
    ita: "it",
    por: "pt",
    jpn: "ja",
    kor: "ko",
  };
  const option = (code: string) => (
    <option key={code} value={code}>
      {names.of(short[code] ?? code)}
    </option>
  );
  return (
    <Card title={t("account.tracks")} description={t("account.tracksHint")}>
      <div className="grid gap-5 sm:grid-cols-2">
        <Field label={t("account.audioLanguage")}>
          <Select
            value={prefs.audio_language ?? ""}
            onChange={(e) => save.mutate({ ...prefs, audio_language: e.target.value || null })}
          >
            <option value="">{t("account.original")}</option>
            {TRACK_LANGUAGES.map(option)}
          </Select>
        </Field>
        <Field label={t("account.subtitleLanguage")}>
          <Select
            value={prefs.subtitle_language ?? ""}
            onChange={(e) => save.mutate({ ...prefs, subtitle_language: e.target.value || null })}
          >
            <option value="">{t("account.sameAsInterface")}</option>
            {TRACK_LANGUAGES.map(option)}
          </Select>
        </Field>
        <Field label={t("account.subtitleMode")}>
          <Select
            value={prefs.subtitle_mode}
            onChange={(e) =>
              save.mutate({ ...prefs, subtitle_mode: e.target.value as Schemas["SubtitleMode"] })
            }
          >
            <option value="foreign">{t("account.modeForeign")}</option>
            <option value="always">{t("account.modeAlways")}</option>
            <option value="never">{t("account.modeNever")}</option>
          </Select>
        </Field>
      </div>
      <p className="mt-4 text-xs text-faint">{t("account.seriesMemoryHint")}</p>
    </Card>
  );
}

function PasswordCard() {
  const { t } = useTranslation();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [done, setDone] = useState(false);
  const change = useMutation({
    mutationFn: () =>
      call(
        api.PUT("/api/me/password", { body: { current_password: current, new_password: next } }),
      ),
    onSuccess: () => {
      setDone(true);
      setCurrent("");
      setNext("");
    },
  });
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setDone(false);
    change.mutate();
  };
  return (
    <Card title={t("account.password")} description={t("account.passwordHint")}>
      <form onSubmit={submit} className="grid gap-4 sm:grid-cols-2">
        <Field label={t("account.currentPassword")}>
          <Input
            type="password"
            autoComplete="current-password"
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
            required
          />
        </Field>
        <Field label={t("account.newPassword")}>
          <Input
            type="password"
            autoComplete="new-password"
            minLength={10}
            value={next}
            onChange={(e) => setNext(e.target.value)}
            required
          />
        </Field>
        <div className="sm:col-span-2">
          <ErrorText>{change.error ? errorText(t, change.error) : null}</ErrorText>
          {done ? (
            <p className="mb-3 text-sm text-success">{t("account.passwordChanged")}</p>
          ) : null}
          <Button type="submit" loading={change.isPending}>
            {t("account.changePassword")}
          </Button>
        </div>
      </form>
    </Card>
  );
}

function TwoFactorCard() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { data: me } = useQuery(meQuery);
  const [stage, setStage] = useState<"idle" | "password" | "scan" | "codes" | "disable">("idle");
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [setup, setSetup] = useState<Schemas["TotpSetup"] | null>(null);
  const [qr, setQr] = useState<string | null>(null);
  const [codes, setCodes] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (setup) void QRCode.toDataURL(setup.otpauth_url, { margin: 1, width: 220 }).then(setQr);
  }, [setup]);

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(errorText(t, e));
    } finally {
      setBusy(false);
    }
  };

  if (!me) return null;
  const enabled = me.totp_enabled;

  return (
    <Card
      title={t("account.twoFactor")}
      description={me.role === "admin" ? t("account.twoFactorAdmin") : t("account.twoFactorHint")}
    >
      <div className="mb-4 flex items-center gap-2">
        <ShieldCheck className={enabled ? "size-5 text-success" : "size-5 text-faint"} />
        <Badge tone={enabled ? "success" : "neutral"}>
          {enabled ? t("account.enabled") : t("account.disabled")}
        </Badge>
      </div>

      {stage === "idle" && !enabled ? (
        <Button variant="primary" onClick={() => setStage("password")}>
          {t("account.enable2fa")}
        </Button>
      ) : null}
      {stage === "idle" && enabled && me.role !== "admin" ? (
        <Button variant="danger" onClick={() => setStage("disable")}>
          {t("account.disable2fa")}
        </Button>
      ) : null}

      {stage === "password" ? (
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            void run(async () => {
              setSetup(await call(api.POST("/api/me/totp/setup", { body: { password } })));
              setStage("scan");
            });
          }}
        >
          <Field label={t("account.confirmPasswordToContinue")}>
            <Input
              type="password"
              autoFocus
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
            />
          </Field>
          <ErrorText>{error}</ErrorText>
          <Button type="submit" variant="primary" loading={busy}>
            {t("common.continue")}
          </Button>
        </form>
      ) : null}

      {stage === "scan" && setup ? (
        <form
          className="flex flex-col gap-5 sm:flex-row"
          onSubmit={(e) => {
            e.preventDefault();
            void run(async () => {
              const res = await call(api.POST("/api/me/totp/enable", { body: { code } }));
              setCodes(res.codes);
              setStage("codes");
              await qc.invalidateQueries({ queryKey: meQuery.queryKey });
            });
          }}
        >
          <motion.div
            initial={{ scale: 0.9, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            className="shrink-0 rounded-xl bg-white p-2"
          >
            {qr ? (
              <img src={qr} alt={t("account.qrAlt")} className="size-44" />
            ) : (
              <div className="size-44" />
            )}
          </motion.div>
          <div className="flex flex-1 flex-col gap-3">
            <p className="text-sm text-muted">{t("account.scanHint")}</p>
            <code className="break-all rounded-lg bg-surface-2 px-3 py-2 font-mono text-xs">
              {setup.secret}
            </code>
            <Field label={t("auth.code")}>
              <Input
                autoFocus
                inputMode="numeric"
                autoComplete="one-time-code"
                value={code}
                onChange={(e) => setCode(e.target.value)}
                className="font-mono tracking-[0.3em]"
                required
              />
            </Field>
            <ErrorText>{error}</ErrorText>
            <Button type="submit" variant="primary" loading={busy}>
              {t("account.activate")}
            </Button>
          </div>
        </form>
      ) : null}

      {stage === "codes" ? (
        <div className="flex flex-col gap-4">
          <p className="text-sm text-muted">{t("account.recoveryHint")}</p>
          <div className="grid grid-cols-2 gap-2 rounded-xl bg-surface-2 p-4 font-mono text-sm">
            {codes.map((c) => (
              <span key={c}>{c}</span>
            ))}
          </div>
          <div className="flex gap-2">
            <Button onClick={() => void navigator.clipboard.writeText(codes.join("\n"))}>
              <Copy className="size-4" />
              {t("common.copy")}
            </Button>
            <Button variant="primary" onClick={() => setStage("idle")}>
              {t("account.savedCodes")}
            </Button>
          </div>
        </div>
      ) : null}

      {stage === "disable" ? (
        <form
          className="grid gap-4 sm:grid-cols-2"
          onSubmit={(e) => {
            e.preventDefault();
            void run(async () => {
              await call(api.POST("/api/me/totp/disable", { body: { password, code } }));
              setStage("idle");
              await qc.invalidateQueries({ queryKey: meQuery.queryKey });
            });
          }}
        >
          <Field label={t("auth.password")}>
            <Input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
            />
          </Field>
          <Field label={t("auth.code")}>
            <Input value={code} onChange={(e) => setCode(e.target.value)} required />
          </Field>
          <div className="sm:col-span-2">
            <ErrorText>{error}</ErrorText>
            <Button type="submit" variant="danger" loading={busy}>
              {t("account.disable2fa")}
            </Button>
          </div>
        </form>
      ) : null}
    </Card>
  );
}

function QuickConnectCard() {
  const { t } = useTranslation();
  const { code: initial = "" } = useSearch({ strict: false }) as { code?: string };
  const [code, setCode] = useState(initial);
  const [done, setDone] = useState(false);
  const approve = useMutation({
    mutationFn: () => call(api.POST("/api/auth/quick-connect/approve", { body: { code } })),
    onSuccess: () => {
      setDone(true);
      setCode("");
    },
  });
  return (
    <Card title={t("account.quickConnect")} description={t("account.quickConnectHint")}>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          setDone(false);
          approve.mutate();
        }}
      >
        <Input
          value={code}
          onChange={(e) => setCode(e.target.value.toUpperCase())}
          maxLength={6}
          placeholder="ABC123"
          className="max-w-44 text-center font-mono text-lg uppercase tracking-[0.3em]"
          required
        />
        <Button type="submit" variant="primary" loading={approve.isPending}>
          {t("account.connect")}
        </Button>
      </form>
      <ErrorText>{approve.error ? errorText(t, approve.error) : null}</ErrorText>
      {done ? <p className="mt-3 text-sm text-success">{t("account.deviceConnected")}</p> : null}
    </Card>
  );
}

const PLATFORM_ICON: Record<string, typeof Laptop> = {
  web: MonitorSmartphone,
  desktop: Laptop,
  android: Smartphone,
  android_tv: Tv,
  other: MonitorSmartphone,
};

function DevicesCard() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const { data: devices } = useQuery(devicesQuery);
  const revoke = useMutation({
    mutationFn: (id: number) =>
      call(api.DELETE("/api/me/devices/{id}", { params: { path: { id } } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: devicesQuery.queryKey }),
  });
  const relative = new Intl.RelativeTimeFormat(i18n.language, { numeric: "auto" });
  const ago = (ts: number) => {
    const minutes = Math.round((ts * 1000 - Date.now()) / 60000);
    if (Math.abs(minutes) < 60) return relative.format(minutes, "minute");
    const hours = Math.round(minutes / 60);
    if (Math.abs(hours) < 48) return relative.format(hours, "hour");
    return relative.format(Math.round(hours / 24), "day");
  };
  return (
    <Card title={t("account.devices")} description={t("account.devicesHint")}>
      <ul className="divide-y divide-line">
        {devices?.map((d, i) => {
          const Icon = PLATFORM_ICON[d.platform] ?? MonitorSmartphone;
          return (
            <motion.li
              key={d.id}
              layout
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: i * 0.04 }}
              className="flex items-center gap-4 py-3"
            >
              <Icon className="size-5 text-muted" />
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium">
                  {d.name}{" "}
                  {d.current ? <Badge tone="accent">{t("account.thisDevice")}</Badge> : null}
                </p>
                <p className="text-xs text-faint">
                  {t("account.lastSeen", { when: ago(d.last_seen_at) })}
                </p>
              </div>
              {!d.current ? (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => revoke.mutate(d.id)}
                  loading={revoke.isPending && revoke.variables === d.id}
                >
                  <Trash2 className="size-4" />
                  {t("account.revoke")}
                </Button>
              ) : null}
            </motion.li>
          );
        })}
      </ul>
    </Card>
  );
}
