import type { LocalItem } from "@batvision/bridge";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { Film, HardDriveDownload, Play, RefreshCw, WifiOff } from "lucide-react";
import { motion } from "motion/react";
import { type FormEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Logo } from "../components/Logo";
import { Img } from "../components/media";
import { Badge, Button, EmptyState, ErrorText, Field, Input, Page } from "../components/ui";
import { desktop, loadDesktopInfo, useDownloadsState } from "../lib/desktop";
import { episodeCode } from "../lib/format";
import { meQuery } from "../lib/queries";
import { AuthShell } from "./AuthPages";

/** First launch of the desktop app: where is the server? */
export function ConnectPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [address, setAddress] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!desktop) return;
    setBusy(true);
    setError(null);
    const result = await desktop.setServer(address);
    setBusy(false);
    if (!result.ok) {
      setError(t(`desktop.serverErrors.${result.error}`));
      return;
    }
    await loadDesktopInfo();
    qc.clear();
    await navigate({ to: "/login" });
  };

  return (
    <AuthShell title={t("desktop.connectTitle")} subtitle={t("desktop.connectIntro")}>
      <form className="flex flex-col gap-4" onSubmit={submit}>
        <Field label={t("desktop.serverAddress")}>
          <Input
            autoFocus
            required
            inputMode="url"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            placeholder={t("desktop.serverPlaceholder")}
            value={address}
            onChange={(e) => setAddress(e.target.value)}
          />
        </Field>
        <ErrorText>{error}</ErrorText>
        <Button type="submit" variant="primary" loading={busy}>
          {t("desktop.connect")}
        </Button>
      </form>
    </AuthShell>
  );
}

/** The server can't be reached: what's on this computer, playable as is. */
export function OfflinePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const state = useDownloadsState();
  const [retrying, setRetrying] = useState(false);
  const done = useMemo(() => (state?.items ?? []).filter((i) => i.status === "done"), [state]);
  const movies = done.filter((i) => i.kind === "movie");
  const series = useMemo(() => groupBySeries(done.filter((i) => i.kind === "episode")), [done]);

  const retry = async () => {
    setRetrying(true);
    qc.removeQueries({ queryKey: meQuery.queryKey });
    try {
      await qc.fetchQuery(meQuery);
      await navigate({ to: "/" });
    } catch {
      // Still unreachable.
    } finally {
      setRetrying(false);
    }
  };

  return (
    <div className="min-h-dvh">
      <header className="sticky top-0 z-30 border-b border-line bg-bg/80 backdrop-blur-xl">
        <div className="mx-auto flex h-16 max-w-[1600px] items-center gap-4 px-4 sm:px-8">
          <Logo />
          <Badge tone="accent">
            <span className="inline-flex items-center gap-1.5">
              <WifiOff className="size-3.5" />
              {t("desktop.offlineTitle")}
            </span>
          </Badge>
          <div className="ml-auto">
            <Button variant="ghost" onClick={retry} loading={retrying}>
              <RefreshCw className="size-4" />
              {t("desktop.retry")}
            </Button>
          </div>
        </div>
      </header>
      <Page className="mx-auto max-w-[1600px] space-y-10 px-4 py-8 sm:px-8">
        <p className="max-w-2xl text-muted">{t("desktop.offlineIntro")}</p>
        {state && done.length === 0 ? (
          <EmptyState
            icon={<HardDriveDownload className="size-7" />}
            title={t("desktop.offlineEmpty")}
          />
        ) : null}
        {movies.length > 0 ? (
          <section>
            <h2 className="mb-4 text-xl font-semibold">{t("desktop.movies")}</h2>
            <div className="grid grid-cols-[repeat(auto-fill,minmax(10rem,1fr))] gap-x-4 gap-y-6">
              {movies.map((item, i) => (
                <OfflineMovie key={item.key} item={item} index={i} />
              ))}
            </div>
          </section>
        ) : null}
        {series.length > 0 ? (
          <section className="space-y-8">
            <h2 className="text-xl font-semibold">{t("desktop.series")}</h2>
            {series.map((group) => (
              <div key={group.title} className="flex flex-col gap-4 sm:flex-row sm:gap-6">
                <Img
                  src={group.poster ?? undefined}
                  alt={group.title}
                  className="aspect-[2/3] w-28 shrink-0 rounded-[var(--radius-card)] object-cover"
                  fallback={<Film className="size-6 text-faint" />}
                />
                <div className="min-w-0 flex-1">
                  <h3 className="mb-3 text-lg font-semibold">{group.title}</h3>
                  <ul className="divide-y divide-line overflow-hidden rounded-xl border border-line">
                    {group.episodes.map((item) => (
                      <OfflineEpisode key={item.key} item={item} />
                    ))}
                  </ul>
                </div>
              </div>
            ))}
          </section>
        ) : null}
      </Page>
    </div>
  );
}

function groupBySeries(episodes: LocalItem[]) {
  const groups = new Map<string, { title: string; poster: string | null; episodes: LocalItem[] }>();
  for (const item of episodes) {
    const key = String(item.meta.seriesId ?? item.meta.title);
    const group = groups.get(key) ?? {
      title: item.meta.title,
      poster: item.meta.poster,
      episodes: [],
    };
    group.episodes.push(item);
    groups.set(key, group);
  }
  const order = (s: number | null) => (s === 0 ? 1e6 : (s ?? 0));
  for (const group of groups.values()) {
    group.episodes.sort(
      (a, b) =>
        order(a.meta.seasonNumber) - order(b.meta.seasonNumber) ||
        (a.meta.episodeNumber ?? 0) - (b.meta.episodeNumber ?? 0),
    );
  }
  return [...groups.values()].sort((a, b) => a.title.localeCompare(b.title));
}

function OfflineMovie({ item, index }: { item: LocalItem; index: number }) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 16 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.4, delay: Math.min(index, 12) * 0.03 }}
    >
      <Link
        to="/offline/watch/$kind/$id"
        params={{ kind: "movie", id: String(item.id) }}
        className="group block outline-none"
      >
        <div className="relative aspect-[2/3] overflow-hidden rounded-[var(--radius-card)] bg-surface-2 shadow-lg ring-1 ring-white/5 transition group-hover:ring-accent/40 group-focus-visible:ring-2 group-focus-visible:ring-accent">
          <Img
            src={item.meta.poster ?? undefined}
            alt={item.meta.title}
            className="size-full object-cover"
            fallback={<Film className="size-7 text-faint" />}
          />
          <span className="absolute inset-0 flex items-center justify-center bg-black/50 opacity-0 transition group-hover:opacity-100">
            <Play className="size-10 fill-current" />
          </span>
        </div>
        <p className="mt-2.5 truncate px-0.5 text-sm font-medium">{item.meta.title}</p>
        {item.meta.year ? <p className="px-0.5 text-xs text-faint">{item.meta.year}</p> : null}
      </Link>
    </motion.div>
  );
}

function OfflineEpisode({ item }: { item: LocalItem }) {
  const { meta } = item;
  return (
    <li>
      <Link
        to="/offline/watch/$kind/$id"
        params={{ kind: "episode", id: String(item.id) }}
        className="flex items-center gap-4 p-3 transition hover:bg-surface-2"
      >
        <div className="relative aspect-video w-36 shrink-0 overflow-hidden rounded-lg bg-surface-2">
          <Img
            src={meta.still ?? undefined}
            alt={meta.episodeTitle ?? meta.title}
            className="size-full object-cover"
            fallback={<Play className="size-5 text-faint" />}
          />
        </div>
        <div className="min-w-0 flex-1">
          <p className="text-xs font-semibold text-accent">
            {episodeCode(meta.seasonNumber, meta.episodeNumber)}
          </p>
          <p className="truncate font-medium">{meta.episodeTitle ?? meta.title}</p>
          {meta.overview ? (
            <p className="line-clamp-2 text-sm text-muted">{meta.overview}</p>
          ) : null}
        </div>
      </Link>
    </li>
  );
}
