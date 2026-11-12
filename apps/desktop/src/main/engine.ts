import { EventEmitter } from "node:events";
import { createWriteStream, existsSync, renameSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import type { DownloadsState, LocalItem, ProgressReport } from "@batvision/bridge";
import type { Config, PendingProgress } from "./config";
import { Library, PARTIAL, type StoredItem, type StoredMeta, VIDEO } from "./library";
import { type Decision, decide, itemKey, type PlanEntry } from "./planner";
import { HttpError, type ServerClient, Unreachable } from "./server";

const SYNC_EVERY_MS = 30 * 60_000;
/** While the server is encoding something for us. */
const SYNC_WAITING_MS = Number(process.env.BATVISION_SYNC_WAITING_MS) || 60_000;
const META_IMAGE_SIZE = { poster: "w342", backdrop: "w1280", still: "w500" } as const;

interface DownloadPlan {
  settings: { quota_bytes?: number | null };
  items: PlanEntry[];
}

type Detail = {
  title?: string | null;
  year?: number | null;
  overview?: string | null;
  poster_path?: string | null;
  backdrop_path?: string | null;
  still_path?: string | null;
  series_id?: number;
  series_title?: string;
  series_poster_path?: string | null;
  series_backdrop_path?: string | null;
  season_number?: number;
  episode_number?: number;
  duration_ms?: number | null;
  runtime_minutes?: number | null;
  position_ms: number;
  played: boolean;
};

const now = () => Math.floor(Date.now() / 1000);

/**
 * Keeps the download folder in line with the server's expected set: uploads
 * offline progress, asks for the plan, deletes, requests, downloads one item at a
 * time (resuming partial files) and confirms.
 */
export class Engine extends EventEmitter {
  library: Library;
  private plan: PlanEntry[] = [];
  private decision: Decision | null = null;
  private quota: number | null = null;
  private online = true;
  private syncing = false;
  private syncAgain = false;
  private lastSyncAt: number | null = null;
  private lastError: string | null = null;
  private downloading: { key: string; received: number; total: number | null } | null = null;
  private failed = new Map<string, string>();
  private abort: AbortController | null = null;
  private loop: Promise<void> | null = null;
  private timer: NodeJS.Timeout | null = null;
  private lastEmit = 0;

  constructor(
    private config: Config,
    private server: ServerClient,
  ) {
    super();
    this.library = new Library(config.folder);
  }

  // ---- State for the interface ---------------------------------------------------------

  state(): DownloadsState {
    const records = this.library.records();
    const items: LocalItem[] = [];
    const seen = new Set<string>();
    for (const entry of this.plan) {
      const key = itemKey(entry.kind, entry.id);
      seen.add(key);
      items.push(this.view(key, entry, records.get(key)?.sizeBytes ?? 0));
    }
    // Downloaded items the last plan didn't mention (offline start, plan not fetched yet).
    for (const stored of this.library.all()) {
      if (!seen.has(stored.key)) items.push(this.view(stored.key, null, stored.sizeBytes));
    }
    return {
      online: this.online,
      syncing: this.syncing,
      paused: this.config.paused,
      lastSyncAt: this.lastSyncAt,
      lastError: this.lastError,
      folder: this.library.folder,
      usedBytes: [...records.values()].reduce((n, r) => n + r.sizeBytes, 0),
      quotaBytes: this.quota,
      items,
    };
  }

  private view(key: string, entry: PlanEntry | null, onDisk: number): LocalItem {
    const stored = this.library.get(key);
    const current = this.downloading?.key === key ? this.downloading : null;
    const status = stored?.complete
      ? "done"
      : current
        ? "downloading"
        : this.failed.has(key) || entry?.status === "failed"
          ? "failed"
          : "waiting";
    return {
      key,
      kind: entry?.kind ?? stored?.kind ?? "movie",
      id: entry?.id ?? stored?.id ?? 0,
      status,
      serverStatus: entry?.status ?? "downloaded",
      encodeProgress: entry?.progress ?? 0,
      receivedBytes: current ? current.received : onDisk,
      totalBytes: stored?.complete
        ? stored.sizeBytes
        : (current?.total ?? entry?.size_bytes ?? null),
      profile: entry?.profile ?? stored?.profile ?? "max",
      audioLanguage: entry?.audio_language ?? stored?.audioLanguage ?? null,
      watched: entry?.watched ?? stored?.played ?? false,
      overQuota: this.decision?.overQuota.has(key) ?? false,
      error: this.failed.get(key) ?? entry?.error ?? null,
      meta: stored ? this.publicMeta(stored) : this.metaFromPlan(entry),
    };
  }

  private metaFromPlan(entry: PlanEntry | null) {
    return {
      title: entry?.title ?? "",
      episodeTitle: entry?.episode_title ?? null,
      seriesId: entry?.series_id ?? null,
      seasonNumber: entry?.season_number ?? null,
      episodeNumber: entry?.episode_number ?? null,
      year: null,
      overview: null,
      poster: entry?.poster_path ? `/api/images/w342${entry.poster_path}` : null,
      backdrop: null,
      still: null,
      durationMs: entry?.duration_ms ?? null,
    };
  }

  /** Stored image names become same-origin URLs served from the item folder. */
  publicMeta(item: StoredItem) {
    const url = (name: string | null) => (name ? `/local/${item.key}/${name}` : null);
    return {
      ...item.meta,
      poster: url(item.meta.poster),
      backdrop: url(item.meta.backdrop),
      still: url(item.meta.still),
    };
  }

  private emitState(force = true) {
    const t = Date.now();
    if (!force && t - this.lastEmit < 500) return;
    this.lastEmit = t;
    this.emit("state", this.state());
  }

  // ---- Lifecycle -----------------------------------------------------------------------

  start() {
    void this.sync();
  }

  private schedule() {
    if (this.timer) clearTimeout(this.timer);
    const waiting = this.plan.some((e) => e.status === "queued" || e.status === "encoding");
    this.timer = setTimeout(() => void this.sync(), waiting ? SYNC_WAITING_MS : SYNC_EVERY_MS);
  }

  stop() {
    if (this.timer) clearTimeout(this.timer);
    this.abort?.abort();
  }

  /** Signed out: nothing to sync until the next sign-in. */
  reset() {
    this.abort?.abort();
    this.plan = [];
    this.decision = null;
    this.failed.clear();
    this.emitState();
  }

  setPaused(paused: boolean) {
    this.config.paused = paused;
    if (paused) this.abort?.abort();
    else this.kick();
    this.emitState();
  }

  setFolder(folder: string) {
    this.abort?.abort();
    this.library.moveTo(folder);
    this.config.folder = folder;
    this.emitState();
    this.kick();
  }

  // ---- Sync ----------------------------------------------------------------------------

  async sync(): Promise<void> {
    if (!this.server.ready) return;
    if (this.syncing) {
      this.syncAgain = true;
      return;
    }
    this.syncing = true;
    this.emitState();
    try {
      await this.flushProgress();
      const av1 = this.config.av1;
      const plan = await this.server.json<DownloadPlan>(
        "/api/downloads/plan",
        av1 === null ? {} : { av1 },
      );
      this.online = true;
      this.plan = plan.items;
      this.quota = plan.settings.quota_bytes ?? null;
      await this.apply();
      await this.refreshPositions();
      this.lastSyncAt = Date.now();
      this.lastError = null;
    } catch (e) {
      this.noteError(e);
    } finally {
      this.syncing = false;
      this.emitState();
      this.schedule();
      this.kick();
    }
    if (this.syncAgain) {
      this.syncAgain = false;
      await this.sync();
    }
  }

  private noteError(e: unknown) {
    if (e instanceof Unreachable) {
      this.online = false;
      this.lastError = null;
    } else {
      this.lastError = e instanceof Error ? e.message : String(e);
      console.error("sync failed:", e);
    }
  }

  private async apply() {
    const decision = decide(this.plan, this.library.records(), this.quota);
    this.decision = decision;
    for (const key of decision.remove) {
      if (this.downloading?.key === key) this.abort?.abort();
      this.library.remove(key);
    }
    for (const action of decision.actions) {
      try {
        if (action.type === "request") {
          await this.server.json("/api/downloads/request", { kind: action.kind, id: action.id });
        } else if (action.type === "confirm") {
          await this.server.json("/api/downloads/confirm", { variant_id: action.variantId });
        }
      } catch (e) {
        if (e instanceof Unreachable) throw e;
        // 409: no longer expected, or the file can't be read. The next plan says so.
        console.warn(`${action.type} ${action.key} failed:`, e);
      }
    }
    // Requests change statuses: show them now rather than at the next sync.
    if (decision.actions.some((a) => a.type === "request")) {
      const fresh = await this.server.json<DownloadPlan>("/api/downloads/plan", {});
      this.plan = fresh.items;
      this.decision = decide(this.plan, this.library.records(), this.quota);
    }
  }

  /** Watched elsewhere since: bring the server's positions to the local copies. */
  private async refreshPositions() {
    for (const item of this.library.all().filter((i) => i.complete)) {
      if (this.config.pendingProgress.some((p) => p.kind === item.kind && p.id === item.id))
        continue;
      try {
        const detail = await this.server.json<Detail>(`/api/${item.kind}s/${item.id}`);
        this.library.put({
          ...item,
          positionMs: detail.position_ms,
          played: detail.played,
          positionAt: now(),
        });
      } catch (e) {
        if (e instanceof Unreachable) throw e;
      }
    }
  }

  // ---- Downloads -----------------------------------------------------------------------

  /** Starts the download loop if it isn't running. */
  kick() {
    if (this.loop || this.config.paused || !this.server.ready) return;
    this.loop = this.runLoop().finally(() => {
      this.loop = null;
    });
  }

  private async runLoop() {
    while (!this.config.paused && this.decision) {
      const next = this.decision.actions.find(
        (a) =>
          a.type === "download" && !this.library.get(a.key)?.complete && !this.failed.has(a.key),
      );
      if (next?.type !== "download") return;
      try {
        await this.download(next.entry);
      } catch (e) {
        if (this.abort?.signal.aborted) return;
        if (e instanceof Unreachable) {
          this.online = false;
          this.emitState();
          return;
        }
        if (e instanceof HttpError && e.status === 404) {
          // Signed URL expired or variant gone: a fresh plan has new URLs.
          void this.sync();
          return;
        }
        this.failed.set(next.key, e instanceof Error ? e.message : String(e));
        console.error(`download ${next.key} failed:`, e);
      } finally {
        this.downloading = null;
        this.abort = null;
        this.emitState();
      }
    }
  }

  private async download(entry: PlanEntry) {
    const key = itemKey(entry.kind, entry.id);
    const dir = this.library.ensureDir(key);
    const partial = join(dir, PARTIAL);
    const total = entry.size_bytes ?? null;
    const stored: StoredItem = this.library.get(key) ?? {
      key,
      kind: entry.kind,
      id: entry.id,
      complete: false,
      variantId: entry.variant_id ?? null,
      profile: entry.profile,
      audioLanguage: entry.audio_language ?? null,
      sizeBytes: total ?? 0,
      subtitles: [],
      meta: { ...this.metaFromPlan(entry), poster: null, backdrop: null, still: null },
      positionMs: 0,
      played: false,
      positionAt: 0,
      downloadedAt: null,
    };
    this.library.put(stored);

    this.abort = new AbortController();
    const signal = this.abort.signal;
    let offset = existsSync(partial) ? statSync(partial).size : 0;
    this.downloading = { key, received: offset, total };
    this.emitState();

    if (total === null || offset < total) {
      const response = await this.server.fetch(
        entry.url as string,
        { headers: offset > 0 ? { range: `bytes=${offset}-` } : {}, signal },
        30_000,
      );
      if (response.status === 404) throw new HttpError(404, "file gone");
      if (!response.ok || !response.body) throw new HttpError(response.status, "download failed");
      // The server ignored the range: start over.
      if (offset > 0 && response.status !== 206) offset = 0;
      const out = createWriteStream(partial, { flags: offset > 0 ? "a" : "w" });
      const body = Readable.fromWeb(response.body as import("node:stream/web").ReadableStream);
      let received = offset;
      body.on("data", (chunk: Buffer) => {
        received += chunk.length;
        if (this.downloading) this.downloading.received = received;
        this.emitState(false);
      });
      await pipeline(body, out, { signal });
    }

    // Subtitles are small: always downloaded whole.
    const subtitles: StoredItem["subtitles"] = [];
    for (const [i, sub] of entry.subtitles.entries()) {
      const response = await this.server.fetch(sub.url, { signal });
      if (!response.ok) throw new HttpError(response.status, "subtitle download failed");
      const file = `sub-${i}.vtt`;
      writeFileSync(join(dir, file), Buffer.from(await response.arrayBuffer()));
      subtitles.push({ file, language: sub.language ?? null, kind: sub.kind });
    }

    const meta = await this.fetchMeta(entry, dir, signal).catch((e) => {
      if (e instanceof Unreachable || signal.aborted) throw e;
      console.warn(`metadata for ${key}:`, e);
      return null;
    });

    renameSync(partial, join(dir, VIDEO));
    const size = statSync(join(dir, VIDEO)).size;
    this.library.put({
      ...stored,
      complete: true,
      sizeBytes: size,
      subtitles,
      meta: meta?.meta ?? stored.meta,
      positionMs: meta?.position ?? 0,
      played: meta?.played ?? false,
      positionAt: now(),
      downloadedAt: now(),
    });
    this.failed.delete(key);
    if (entry.variant_id) {
      await this.server
        .json("/api/downloads/confirm", { variant_id: entry.variant_id })
        .catch((e) => console.warn(`confirm ${key} failed (retried at next sync):`, e));
    }
  }

  private async fetchMeta(entry: PlanEntry, dir: string, signal: AbortSignal) {
    const d = await this.server.json<Detail>(`/api/${entry.kind}s/${entry.id}`);
    const image = async (path: string | null | undefined, size: string, name: string) => {
      if (!path) return null;
      try {
        const response = await this.server.fetch(`/api/images/${size}${path}`, { signal });
        if (!response.ok) return null;
        const file = `${name}${path.slice(path.lastIndexOf("."))}`;
        writeFileSync(join(dir, file), Buffer.from(await response.arrayBuffer()));
        return file;
      } catch (e) {
        if (e instanceof Unreachable || signal.aborted) throw e;
        return null;
      }
    };
    const episode = entry.kind === "episode";
    const meta: StoredMeta = {
      title: (episode ? d.series_title : d.title) ?? entry.title,
      episodeTitle: episode ? (d.title ?? null) : null,
      seriesId: d.series_id ?? null,
      seasonNumber: d.season_number ?? null,
      episodeNumber: d.episode_number ?? null,
      year: d.year ?? null,
      overview: d.overview ?? null,
      poster: await image(
        episode ? d.series_poster_path : d.poster_path,
        META_IMAGE_SIZE.poster,
        "poster",
      ),
      backdrop: await image(
        episode ? d.series_backdrop_path : d.backdrop_path,
        META_IMAGE_SIZE.backdrop,
        "backdrop",
      ),
      still: episode ? await image(d.still_path, META_IMAGE_SIZE.still, "still") : null,
      durationMs:
        entry.duration_ms ??
        d.duration_ms ??
        (d.runtime_minutes ? d.runtime_minutes * 60_000 : null),
    };
    return { meta, position: d.position_ms, played: d.played };
  }

  // ---- Offline playback ----------------------------------------------------------------

  localPlayback(kind: "movie" | "episode", id: number) {
    const item = this.library.get(itemKey(kind, id));
    if (!item?.complete) return null;
    return {
      url: `/local/${item.key}/${VIDEO}`,
      subtitles: item.subtitles.map((s) => ({
        url: `/local/${item.key}/${s.file}`,
        language: s.language,
        kind: s.kind,
      })),
      durationMs: item.meta.durationMs,
      positionMs: item.played ? 0 : item.positionMs,
      audioLanguage: item.audioLanguage,
      nextEpisodeId: this.nextLocalEpisode(item),
      meta: this.publicMeta(item),
    };
  }

  private nextLocalEpisode(item: StoredItem): number | null {
    const { seriesId, seasonNumber, episodeNumber } = item.meta;
    if (item.kind !== "episode" || seriesId === null) return null;
    // Specials (season 0) come after the regular seasons.
    const order = (s: number | null) => (s === 0 ? 1e6 : (s ?? 0));
    const rank = (i: StoredItem) => order(i.meta.seasonNumber) * 1e4 + (i.meta.episodeNumber ?? 0);
    const current = order(seasonNumber) * 1e4 + (episodeNumber ?? 0);
    const next = this.library
      .all()
      .filter((i) => i.complete && i.kind === "episode" && i.meta.seriesId === seriesId)
      .filter((i) => rank(i) > current)
      .sort((a, b) => rank(a) - rank(b))[0];
    return next?.id ?? null;
  }

  reportProgress(report: ProgressReport) {
    const key = itemKey(report.kind, report.id);
    const item = this.library.get(key);
    if (item) {
      item.positionMs = report.positionMs;
      item.played = item.played || report.played;
      item.positionAt = now();
      this.library.put(item);
    }
    this.config.addProgress({
      kind: report.kind,
      id: report.id,
      position_ms: Math.round(report.positionMs),
      played: report.played,
      at: now(),
    });
    this.scheduleFlush();
  }

  private flushTimer: NodeJS.Timeout | null = null;

  private scheduleFlush() {
    if (this.flushTimer) return;
    this.flushTimer = setTimeout(() => {
      this.flushTimer = null;
      void this.flushProgress().catch((e) => this.noteError(e));
    }, 5_000);
  }

  /** Sends offline progress; kept for later when the server can't be reached. */
  async flushProgress() {
    const pending: readonly PendingProgress[] = this.config.pendingProgress;
    if (pending.length === 0 || !this.server.ready) return;
    const batch = pending.slice(0, 1000);
    await this.server.json("/api/progress/batch", batch);
    this.config.removeProgress(batch);
    this.online = true;
  }
}
