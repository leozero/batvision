import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Bookmark, DownloadCloud, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { PosterCard, PosterGrid } from "../components/media";
import { Button, cx, EmptyState, Page, Skeleton } from "../components/ui";
import { api, call, type Schemas } from "../lib/api";
import { syncDesktop } from "../lib/desktop";
import {
  devicesQuery,
  downloadsQuery,
  invalidateLibrary,
  preferencesQuery,
  watchlistQuery,
} from "../lib/queries";

type Filter = "all" | "movie" | "series";

/** Is the watchlist downloaded here, or anywhere (D42, D73)? */
function AutoBanner() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { data: overview } = useQuery(downloadsQuery);
  const { data: devices } = useQuery(devicesQuery);
  const currentId = devices?.find((d) => d.current)?.id;
  const here = overview?.devices.find((d) => d.id === currentId);
  const enable = useMutation({
    mutationFn: (device: Schemas["DeviceDownloads"]) =>
      call(
        api.PUT("/api/downloads/devices/{id}/settings", {
          params: { path: { id: device.id } },
          body: { ...device.settings, auto_download: true },
        }),
      ),
    onSuccess: () => {
      syncDesktop();
      return qc.invalidateQueries({ queryKey: downloadsQuery.queryKey });
    },
  });
  if (!overview) return null;
  // In a download app with automatic downloads off: offer to turn them on here.
  if (here && !here.settings.auto_download) {
    return (
      <div
        data-testid="auto-off"
        className="mb-8 flex flex-wrap items-center gap-4 rounded-2xl border border-accent/30 bg-accent/10 p-4 text-sm"
      >
        <DownloadCloud className="size-5 shrink-0 text-accent" />
        <p className="flex-1">{t("watchlist.autoOffHere")}</p>
        <Button variant="primary" loading={enable.isPending} onClick={() => enable.mutate(here)}>
          {t("watchlist.enableHere")}
        </Button>
      </div>
    );
  }
  const count = overview.devices.filter((d) => d.settings.auto_download).length;
  return (
    <p data-testid="auto-devices" className="mb-8 text-sm text-muted">
      {count ? t("watchlist.autoDevices", { count }) : t("watchlist.autoNone")}
    </p>
  );
}

export function WatchlistPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { data: items, isPending } = useQuery(watchlistQuery);
  const { data: prefs } = useQuery(preferencesQuery);
  const [filter, setFilter] = useState<Filter>("all");
  const remove = useMutation({
    mutationFn: (item: Schemas["WatchlistItem"]) =>
      call(
        item.kind === "movie"
          ? api.DELETE("/api/watchlist/{movie_id}", { params: { path: { movie_id: item.id } } })
          : api.DELETE("/api/watchlist/series/{series_id}", {
              params: { path: { series_id: item.id } },
            }),
      ),
    onSuccess: () => {
      syncDesktop();
      invalidateLibrary(qc);
      void qc.invalidateQueries({ queryKey: downloadsQuery.queryKey });
      return qc.invalidateQueries({ queryKey: watchlistQuery.queryKey });
    },
  });
  const shown = (items ?? []).filter((i) => filter === "all" || i.kind === filter);
  const filters: { value: Filter; label: string }[] = [
    { value: "all", label: t("watchlist.all") },
    { value: "movie", label: t("watchlist.movies") },
    { value: "series", label: t("watchlist.series") },
  ];

  return (
    <Page className="mx-auto max-w-[1800px] px-4 pt-6 sm:px-10">
      <h1 className="text-3xl font-bold tracking-tight">{t("watchlist.title")}</h1>
      <p className="mt-2 max-w-2xl text-sm text-muted">{t("watchlist.intro")}</p>
      {prefs ? (
        <p className="mt-1 text-sm text-muted">
          {t("watchlist.episodes", { count: prefs.watchlist_episodes })} ·{" "}
          <Link
            to="/account"
            search={{ tab: "playback" }}
            className="text-accent hover:underline"
            data-testid="episodes-setting"
          >
            {t("watchlist.episodesSetting")}
          </Link>
        </p>
      ) : null}
      <div className="mt-4">
        <AutoBanner />
      </div>

      <div className="mb-6 flex gap-2" role="tablist">
        {filters.map((f) => (
          <button
            key={f.value}
            type="button"
            role="tab"
            aria-selected={filter === f.value}
            onClick={() => setFilter(f.value)}
            className={cx(
              "cursor-pointer rounded-full px-4 py-1.5 text-sm font-medium transition-colors",
              filter === f.value ? "bg-fg text-bg" : "bg-surface-2 text-muted hover:text-fg",
            )}
          >
            {f.label}
          </button>
        ))}
      </div>

      {isPending ? (
        <PosterGrid>
          {Array.from({ length: 12 }, (_, i) => (
            <Skeleton key={i} className="aspect-[2/3]" />
          ))}
        </PosterGrid>
      ) : shown.length === 0 ? (
        <EmptyState icon={<Bookmark className="size-8" />} title={t("watchlist.empty")}>
          <p>{t("watchlist.emptyHint")}</p>
        </EmptyState>
      ) : (
        <PosterGrid>
          {shown.map((item, i) => (
            <div key={`${item.kind}-${item.id}`} className="group/item relative">
              <PosterCard item={item} kind={item.kind} index={i} />
              <button
                type="button"
                aria-label={`${t("downloads.remove")} ${item.title}`}
                disabled={remove.isPending}
                onClick={() => remove.mutate(item)}
                // Always shown on touch screens: hidden, it still took the taps meant for the poster.
                className="absolute right-2 top-2 flex size-8 cursor-pointer items-center justify-center rounded-full bg-black/75 transition focus-visible:opacity-100 pointer-fine:opacity-0 pointer-fine:group-hover/item:opacity-100"
              >
                <Trash2 className="size-4" />
              </button>
            </div>
          ))}
        </PosterGrid>
      )}
    </Page>
  );
}
