import type { DesktopSettings, LocalItem } from "@batvision/bridge";
import { useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import type { TFunction } from "i18next";
import {
  CheckCircle2,
  Clock,
  FolderOpen,
  HardDriveDownload,
  Loader2,
  Pause,
  Play,
  RefreshCw,
  WifiOff,
  X,
  XCircle,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatBytes } from "../lib/bytes";
import { desktop, useDownloadsState, useLocalItem } from "../lib/desktop";
import { episodeCode } from "../lib/format";
import { downloadsQuery } from "../lib/queries";
import { Card } from "../pages/account/AccountPage";
import { Badge, Button, Dialog, ErrorText, IconButton, ProgressBar } from "./ui";

/** "Downloaded" on a detail page, when this computer has the item. */
export function DownloadedMark({ kind, id }: { kind: "movie" | "episode"; id: number }) {
  const { t } = useTranslation();
  const item = useLocalItem(kind, id);
  if (item?.status !== "done") return null;
  return (
    <span className="inline-flex items-center gap-1 text-accent">
      <HardDriveDownload className="size-4" /> {t("desktop.downloaded")}
    </span>
  );
}

function relativeTime(ms: number, lang: string) {
  const seconds = Math.round((ms - Date.now()) / 1000);
  const rtf = new Intl.RelativeTimeFormat(lang, { numeric: "auto" });
  if (Math.abs(seconds) < 60) return rtf.format(seconds, "second");
  if (Math.abs(seconds) < 3600) return rtf.format(Math.round(seconds / 60), "minute");
  if (Math.abs(seconds) < 86_400) return rtf.format(Math.round(seconds / 3600), "hour");
  return rtf.format(Math.round(seconds / 86_400), "day");
}

function ItemState({ item, paused }: { item: LocalItem; paused: boolean }) {
  const { t, i18n } = useTranslation();
  const line = (icon: ReactNode, text: string, tone = "text-muted") => (
    <span className={`inline-flex items-center gap-1.5 text-xs ${tone}`}>
      {icon}
      {text}
    </span>
  );
  if (item.status === "done")
    return line(<CheckCircle2 className="size-3.5" />, t("desktop.state.done"), "text-success");
  if (item.overQuota) return line(<XCircle className="size-3.5" />, t("desktop.state.overQuota"));
  if (item.status === "failed")
    return line(<XCircle className="size-3.5" />, t("desktop.state.failed"), "text-danger");
  if (item.status === "downloading")
    return line(
      <Loader2 className="size-3.5 animate-spin" />,
      t("desktop.state.downloading", {
        received: formatBytes(item.receivedBytes, i18n.language),
        total: formatBytes(item.totalBytes, i18n.language),
      }),
      "text-accent",
    );
  if (item.serverStatus === "encoding")
    return line(
      <Loader2 className="size-3.5 animate-spin" />,
      t("desktop.state.encoding", { percent: Math.round(item.encodeProgress * 100) }),
      "text-accent",
    );
  if (item.serverStatus === "queued")
    return line(<Clock className="size-3.5" />, t("desktop.state.queued"));
  if (paused && item.serverStatus === "ready")
    return line(<Pause className="size-3.5" />, t("desktop.state.paused"));
  return line(<Clock className="size-3.5" />, t("desktop.state.waiting"));
}

/** What cancelling removes, said before doing it (D74). */
export function cancelEffect(t: TFunction, item: LocalItem) {
  const source = item.source ?? "manual";
  return t(`downloads.effect.${source}`, { title: item.meta.title });
}

/** Cancels a download in progress, or deletes a finished one, after confirming. */
function CancelButton({ item }: { item: LocalItem }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const done = item.status === "done";
  const confirm = async () => {
    setBusy(true);
    setError(false);
    try {
      await desktop?.cancel(item.kind, item.id);
      setOpen(false);
      void qc.invalidateQueries();
    } catch {
      setError(true);
    } finally {
      setBusy(false);
    }
  };
  if (!desktop?.cancel) return null;
  return (
    <>
      <IconButton
        label={done ? t("downloads.delete") : t("downloads.cancel")}
        data-testid={`cancel-${item.key}`}
        className="size-9"
        onClick={() => setOpen(true)}
      >
        <X className="size-4" />
      </IconButton>
      <Dialog
        open={open}
        onOpenChange={setOpen}
        title={done ? t("downloads.deleteTitle") : t("downloads.cancelTitle")}
        description={cancelEffect(t, item)}
      >
        <ErrorText>{error ? t("downloads.cancelFailed") : null}</ErrorText>
        <div className="mt-2 flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setOpen(false)}>
            {t("downloads.keep")}
          </Button>
          <Button variant="danger" loading={busy} onClick={() => void confirm()}>
            {done ? t("downloads.delete") : t("downloads.cancel")}
          </Button>
        </div>
      </Dialog>
    </>
  );
}

/** Desktop app only: what this computer holds, and its download settings. */
export function ThisComputerCard() {
  const { t, i18n } = useTranslation();
  const state = useDownloadsState();
  const [settings, setSettings] = useState<DesktopSettings>();
  useEffect(() => {
    void desktop?.settings().then(setSettings);
  }, []);
  // The server's view of this device changes when a sync or a download ends.
  const qc = useQueryClient();
  const done = state?.items.filter((i) => i.status === "done").length;
  const lastSync = state?.lastSyncAt;
  useEffect(() => {
    if (done !== undefined || lastSync)
      void qc.invalidateQueries({ queryKey: downloadsQuery.queryKey });
  }, [done, lastSync, qc]);
  if (!desktop || !state) return null;

  const used = formatBytes(state.usedBytes, i18n.language);
  const ratio = state.quotaBytes ? Math.min(1, state.usedBytes / state.quotaBytes) : 0;

  return (
    <Card title={t("desktop.thisComputer")}>
      <div className="space-y-5">
        <div className="flex flex-wrap items-center gap-3">
          {state.online ? null : (
            <Badge tone="accent">
              <span className="inline-flex items-center gap-1.5">
                <WifiOff className="size-3.5" />
                {t("desktop.serverUnreachable")}
              </span>
            </Badge>
          )}
          <span className="text-xs text-faint">
            {state.syncing
              ? t("desktop.syncing")
              : state.lastSyncAt
                ? t("desktop.lastSync", { time: relativeTime(state.lastSyncAt, i18n.language) })
                : null}
          </span>
          <div className="ml-auto flex gap-2">
            <Button variant="ghost" onClick={() => void desktop?.setPaused(!state.paused)}>
              {state.paused ? <Play className="size-4" /> : <Pause className="size-4" />}
              {state.paused ? t("desktop.resume") : t("desktop.pause")}
            </Button>
            <Button variant="ghost" loading={state.syncing} onClick={() => void desktop?.syncNow()}>
              <RefreshCw className="size-4" />
              {t("desktop.syncNow")}
            </Button>
          </div>
        </div>

        <div>
          <p className="mb-2 text-sm">
            {state.quotaBytes
              ? t("desktop.used", { used, quota: formatBytes(state.quotaBytes, i18n.language) })
              : t("desktop.usedNoQuota", { used })}
          </p>
          {state.quotaBytes ? <ProgressBar value={ratio} /> : null}
        </div>

        {state.items.length ? (
          <ul className="divide-y divide-line overflow-hidden rounded-xl border border-line">
            {state.items.map((item) => (
              <li key={item.key} className="flex items-center gap-3 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">
                    {item.meta.title}
                    {item.kind === "episode" ? (
                      <span className="text-muted">
                        {" "}
                        · {episodeCode(item.meta.seasonNumber, item.meta.episodeNumber)}
                        {item.meta.episodeTitle ? ` ${item.meta.episodeTitle}` : ""}
                      </span>
                    ) : null}
                  </p>
                  <ItemState item={item} paused={state.paused} />
                  {item.status === "downloading" && item.totalBytes ? (
                    <ProgressBar
                      value={item.receivedBytes / item.totalBytes}
                      className="mt-2 max-w-sm"
                    />
                  ) : null}
                </div>
                {item.status === "done" ? (
                  <Link
                    to="/watch/$kind/$id"
                    params={{ kind: item.kind, id: String(item.id) }}
                    aria-label={item.meta.title}
                    className="flex size-9 items-center justify-center rounded-full bg-white/8 transition hover:bg-accent hover:text-accent-ink"
                  >
                    <Play className="size-4 fill-current" />
                  </Link>
                ) : null}
                <CancelButton item={item} />
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-sm text-muted">{t("desktop.empty")}</p>
        )}

        {settings ? (
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="rounded-xl bg-surface-2 px-4 py-3">
              <p className="text-sm font-medium">{t("desktop.folder")}</p>
              <p className="truncate text-xs text-faint" title={settings.folder}>
                {settings.folder}
              </p>
              <div className="mt-2 flex gap-2">
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={async () => setSettings(await desktop?.chooseFolder())}
                >
                  {t("desktop.chooseFolder")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => void desktop?.openFolder()}>
                  <FolderOpen className="size-4" />
                  {t("desktop.openFolder")}
                </Button>
              </div>
            </div>
            <label className="flex cursor-pointer items-center justify-between gap-4 rounded-xl bg-surface-2 px-4 py-3">
              <span>
                <span className="block text-sm font-medium">{t("desktop.launchAtLogin")}</span>
                <span className="text-xs text-faint">{t("desktop.launchAtLoginHint")}</span>
              </span>
              <input
                type="checkbox"
                className="size-5 accent-[var(--color-accent)]"
                checked={settings.launchAtLogin}
                onChange={async (e) =>
                  setSettings(await desktop?.setLaunchAtLogin(e.target.checked))
                }
              />
            </label>
          </div>
        ) : null}
      </div>
    </Card>
  );
}
