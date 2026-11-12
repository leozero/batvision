import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  Bookmark,
  CheckCircle2,
  Clock,
  Download,
  Laptop,
  Loader2,
  MonitorSmartphone,
  Smartphone,
  Trash2,
  Tv,
  XCircle,
} from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Img } from "../components/media";
import { ThisComputerCard } from "../components/ThisComputer";
import {
  Badge,
  Button,
  EmptyState,
  Field,
  Input,
  Page,
  ProgressBar,
  Select,
  Skeleton,
} from "../components/ui";
import { api, call, imageUrl, type Schemas } from "../lib/api";
import { formatBytes } from "../lib/bytes";
import { desktop, syncDesktop } from "../lib/desktop";
import { episodeCode, languageName } from "../lib/format";
import { downloadsQuery } from "../lib/queries";
import { Card } from "./account/AccountPage";

const PLATFORM_ICON: Record<string, typeof Laptop> = {
  desktop: Laptop,
  android: Smartphone,
  android_tv: Tv,
  other: MonitorSmartphone,
};

const GB = 1_000_000_000;

function StatusBadge({ item }: { item: Schemas["PlanItem"] }) {
  const { t } = useTranslation();
  switch (item.status) {
    case "downloaded":
      return (
        <span className="inline-flex items-center gap-1 text-xs text-success">
          <CheckCircle2 className="size-3.5" /> {t("downloads.status.downloaded")}
        </span>
      );
    case "ready":
      return <Badge tone="accent">{t("downloads.status.ready")}</Badge>;
    case "encoding":
      return (
        <span className="inline-flex items-center gap-1.5 text-xs text-accent">
          <Loader2 className="size-3.5 animate-spin" />
          {t("downloads.status.encoding", { percent: Math.round(item.progress * 100) })}
        </span>
      );
    case "queued":
      return (
        <span className="inline-flex items-center gap-1 text-xs text-muted">
          <Clock className="size-3.5" /> {t("downloads.status.queued")}
        </span>
      );
    case "failed":
      return (
        <span
          className="inline-flex items-center gap-1 text-xs text-danger"
          title={item.error ?? ""}
        >
          <XCircle className="size-3.5" /> {t("downloads.status.failed")}
        </span>
      );
    case "unavailable":
      return <span className="text-xs text-faint">{t("downloads.status.unavailable")}</span>;
    default:
      return <span className="text-xs text-faint">{t("downloads.status.notRequested")}</span>;
  }
}

function SettingsForm({ device }: { device: Schemas["DeviceDownloads"] }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [s, setS] = useState(device.settings);
  useEffect(() => setS(device.settings), [device.settings]);
  const save = useMutation({
    mutationFn: (settings: Schemas["DeviceSettings"]) =>
      call(
        api.PUT("/api/downloads/devices/{id}/settings", {
          params: { path: { id: device.id } },
          body: settings,
        }),
      ),
    onSuccess: () => {
      syncDesktop();
      return qc.invalidateQueries({ queryKey: downloadsQuery.queryKey });
    },
  });
  const update = (patch: Partial<Schemas["DeviceSettings"]>) => {
    const next = { ...s, ...patch };
    setS(next);
    save.mutate(next);
  };
  const gb = (bytes: number | null | undefined) => (bytes ? String(Math.round(bytes / GB)) : "");
  const isAndroid = device.platform === "android" || device.platform === "android_tv";
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <label className="flex cursor-pointer items-center justify-between gap-4 rounded-xl bg-surface-2 px-4 py-3 sm:col-span-2">
        <span>
          <span className="block text-sm font-medium">{t("downloads.auto")}</span>
          <span className="text-xs text-faint">{t("downloads.autoHint")}</span>
        </span>
        <input
          type="checkbox"
          className="size-5 accent-[var(--color-accent)]"
          checked={s.auto_download}
          onChange={(e) => update({ auto_download: e.target.checked })}
        />
      </label>
      <Field label={t("downloads.quality")}>
        <Select
          value={s.profile}
          onChange={(e) => update({ profile: e.target.value as Schemas["Profile"] })}
        >
          <option value="max">{t("downloads.profile.max")}</option>
          <option value="mobile">{t("downloads.profile.mobile")}</option>
          <option value="saver">{t("downloads.profile.saver")}</option>
        </Select>
      </Field>
      <Field label={t("downloads.quota")} hint={t("downloads.quotaHint")}>
        <Input
          type="number"
          min={1}
          inputMode="numeric"
          defaultValue={gb(s.quota_bytes)}
          placeholder="∞"
          onBlur={(e) =>
            update({ quota_bytes: e.target.value ? Number(e.target.value) * GB : null })
          }
        />
      </Field>
      {isAndroid ? (
        <>
          <label className="flex cursor-pointer items-center justify-between gap-4 rounded-xl bg-surface-2 px-4 py-3">
            <span className="text-sm font-medium">{t("downloads.mobileData")}</span>
            <input
              type="checkbox"
              className="size-5 accent-[var(--color-accent)]"
              checked={s.mobile_data}
              onChange={(e) => update({ mobile_data: e.target.checked })}
            />
          </label>
          <Field label={t("downloads.mobileCap")}>
            <Input
              type="number"
              min={1}
              disabled={!s.mobile_data}
              defaultValue={gb(s.mobile_cap_bytes)}
              placeholder="2"
              onBlur={(e) =>
                update({ mobile_cap_bytes: e.target.value ? Number(e.target.value) * GB : null })
              }
            />
          </Field>
          <Field label={t("downloads.storage")}>
            <Select
              value={s.storage}
              onChange={(e) => update({ storage: e.target.value as Schemas["Storage"] })}
            >
              <option value="internal">{t("downloads.storageInternal")}</option>
              <option value="sd">{t("downloads.storageSd")}</option>
            </Select>
          </Field>
        </>
      ) : null}
    </div>
  );
}

function DeviceCard({ device, index }: { device: Schemas["DeviceDownloads"]; index: number }) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const Icon = PLATFORM_ICON[device.platform] ?? MonitorSmartphone;
  const removeSub = useMutation({
    mutationFn: (id: number) =>
      call(api.DELETE("/api/downloads/subscriptions/{id}", { params: { path: { id } } })),
    onSuccess: () => {
      syncDesktop();
      return qc.invalidateQueries({ queryKey: downloadsQuery.queryKey });
    },
  });
  const removeManual = useMutation({
    mutationFn: (id: number) =>
      call(api.DELETE("/api/downloads/manual/{id}", { params: { path: { id } } })),
    onSuccess: () => {
      syncDesktop();
      return qc.invalidateQueries({ queryKey: downloadsQuery.queryKey });
    },
  });
  const total = device.items.reduce((n, i) => n + (i.size_bytes ?? 0), 0);
  return (
    <motion.section
      initial={{ opacity: 0, y: 16 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: index * 0.06 }}
      className="rounded-2xl border border-line bg-surface p-6"
    >
      <header className="mb-5 flex items-center gap-3">
        <span className="flex size-11 items-center justify-center rounded-xl bg-surface-2 text-accent">
          <Icon className="size-5" />
        </span>
        <div className="flex-1">
          <h2 className="font-semibold">{device.name}</h2>
          <p className="text-xs text-faint">
            {t("downloads.itemsSummary", {
              count: device.items.length,
              size: formatBytes(total, i18n.language),
            })}
          </p>
        </div>
      </header>

      <SettingsForm device={device} />

      {device.subscriptions.length > 0 || device.manual.length > 0 ? (
        <div className="mt-6">
          <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-faint">
            {t("downloads.requests")}
          </h3>
          <ul className="divide-y divide-line">
            {device.subscriptions.map((s) => (
              <li key={`s${s.id}`} className="flex items-center gap-3 py-2.5 text-sm">
                <Tv className="size-4 text-muted" />
                <span className="flex-1">
                  <span className="font-medium">{s.series_title}</span>{" "}
                  <span className="text-muted">
                    ·{" "}
                    {s.mode === "rolling"
                      ? t("downloads.nextEpisodes", { count: s.count ?? 3 })
                      : t("detail.season", { number: s.season_number })}
                    {s.audio_language ? ` · ${languageName(s.audio_language, i18n.language)}` : ""}
                  </span>
                </span>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => removeSub.mutate(s.id)}
                  aria-label={t("downloads.remove")}
                >
                  <Trash2 className="size-4" />
                </Button>
              </li>
            ))}
            {device.manual.map((m) => (
              <li key={`m${m.id}`} className="flex items-center gap-3 py-2.5 text-sm">
                <Download className="size-4 text-muted" />
                <span className="flex-1 font-medium">{m.title}</span>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => removeManual.mutate(m.id)}
                  aria-label={t("downloads.remove")}
                >
                  <Trash2 className="size-4" />
                </Button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <div className="mt-6">
        <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-faint">
          {t("downloads.onDevice")}
        </h3>
        {device.items.length === 0 ? (
          <p className="text-sm text-muted">{t("downloads.nothingExpected")}</p>
        ) : (
          <ul className="space-y-2">
            {device.items.map((item) => (
              <li
                key={`${item.kind}-${item.id}`}
                className="flex items-center gap-3 rounded-xl bg-surface-2/60 p-2"
              >
                <Img
                  src={imageUrl(item.poster_path, "w185")}
                  alt=""
                  className="h-14 w-10 shrink-0 rounded-md object-cover"
                />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">
                    {item.title}
                    {item.kind === "episode" ? (
                      <span className="text-muted">
                        {" "}
                        · {episodeCode(item.season_number, item.episode_number)}
                      </span>
                    ) : null}
                  </p>
                  <div className="mt-0.5 flex flex-wrap items-center gap-2">
                    <StatusBadge item={item} />
                    <span className="text-xs text-faint">
                      {t(`downloads.profile.${item.profile}`)}
                      {item.size_bytes ? ` · ${formatBytes(item.size_bytes, i18n.language)}` : ""}
                      {item.watched ? ` · ${t("downloads.watched")}` : ""}
                    </span>
                  </div>
                  {item.status === "encoding" ? (
                    <ProgressBar value={item.progress} className="mt-1.5" />
                  ) : null}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </motion.section>
  );
}

export function DownloadsPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { data, isPending } = useQuery(downloadsQuery);
  const removeFromWatchlist = useMutation({
    mutationFn: (movie_id: number) =>
      call(api.DELETE("/api/watchlist/{movie_id}", { params: { path: { movie_id } } })),
    onSuccess: () => {
      syncDesktop();
      return qc.invalidateQueries();
    },
  });

  return (
    <Page className="mx-auto max-w-4xl px-4 pt-8 sm:px-6">
      <h1 className="mb-2 text-3xl font-bold tracking-tight">{t("downloads.title")}</h1>
      <p className="mb-8 text-sm text-muted">{t("downloads.intro")}</p>

      {isPending ? (
        <Skeleton className="h-64" />
      ) : (
        <div className="space-y-6">
          {desktop ? <ThisComputerCard /> : null}
          <Card title={t("downloads.watchlist")} description={t("downloads.watchlistHint")}>
            {data?.watchlist.length ? (
              <ul className="grid grid-cols-[repeat(auto-fill,minmax(7rem,1fr))] gap-4">
                {data.watchlist.map((w) => (
                  <li key={w.movie_id} className="group relative">
                    <Link to="/movies/$id" params={{ id: String(w.movie_id) }}>
                      <Img
                        src={imageUrl(w.poster_path, "w342")}
                        alt={w.title}
                        className="aspect-[2/3] w-full rounded-lg object-cover"
                        fallback={<Bookmark className="size-6 text-faint" />}
                      />
                      <p className="mt-1.5 truncate text-xs font-medium">{w.title}</p>
                    </Link>
                    <button
                      type="button"
                      aria-label={t("downloads.remove")}
                      onClick={() => removeFromWatchlist.mutate(w.movie_id)}
                      className="absolute right-1.5 top-1.5 flex size-7 cursor-pointer items-center justify-center rounded-full bg-black/70 opacity-0 transition group-hover:opacity-100"
                    >
                      <Trash2 className="size-3.5" />
                    </button>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-sm text-muted">{t("downloads.watchlistEmpty")}</p>
            )}
          </Card>

          {data?.devices.length ? (
            data.devices.map((device, i) => (
              <DeviceCard key={device.id} device={device} index={i} />
            ))
          ) : (
            <EmptyState icon={<Download className="size-8" />} title={t("downloads.noDeviceTitle")}>
              <p>{t("downloads.noDevice")}</p>
            </EmptyState>
          )}
        </div>
      )}
    </Page>
  );
}
