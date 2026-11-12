/**
 * Contract between the desktop app (Electron main + preload) and the shared web
 * interface. On the web, `window.batvision` is undefined and none of this exists.
 */

export type ItemKind = "movie" | "episode";

/** How this install presents itself to the server when signing in. */
export interface DeviceInfo {
  name: string;
  platform: "desktop";
}

export interface DesktopInfo {
  version: string;
  os: "mac" | "windows" | "linux";
  /** Server origin (`https://media.example.com`), or `null` before the first setup. */
  server: string | null;
  signedIn: boolean;
  device: DeviceInfo;
}

export type SetServerResult =
  | { ok: true }
  | { ok: false; error: "invalid_url" | "unreachable" | "not_batvision" };

export interface LocalSubtitle {
  /** Same-origin URL of the WebVTT file. */
  url: string;
  language: string | null;
  /** Server `TrackKind`: `normal`, `forced`, `sdh`, `commentary`… */
  kind: string;
}

/** What the offline library shows about a downloaded item. */
export interface LocalMeta {
  /** Movie title, or series title for an episode. */
  title: string;
  episodeTitle: string | null;
  seriesId: number | null;
  seasonNumber: number | null;
  episodeNumber: number | null;
  year: number | null;
  overview: string | null;
  /** Same-origin URLs of images saved next to the files. */
  poster: string | null;
  backdrop: string | null;
  still: string | null;
  durationMs: number | null;
}

export type LocalStatus =
  /** Waiting for the server (encoding) or for its turn. */
  "waiting" | "downloading" | "done" | "failed";

export interface LocalItem {
  /** `movie-12`, `episode-34`. */
  key: string;
  kind: ItemKind;
  id: number;
  status: LocalStatus;
  /** Server-side state from the last plan (`not_requested`, `queued`, `encoding`, `ready`…). */
  serverStatus: string;
  /** Server encoding progress, 0 to 1. */
  encodeProgress: number;
  receivedBytes: number;
  totalBytes: number | null;
  profile: string;
  audioLanguage: string | null;
  /** Watched: deleted after the keep window. */
  watched: boolean;
  /** Doesn't fit in the quota: kept on the server's list, not downloaded. */
  overQuota: boolean;
  /** Why the server expects it: `manual`, `rolling`, `season`, `watchlist`; `null` when
   * the last plan didn't mention it. Says what cancelling removes (D74). */
  source: string | null;
  error: string | null;
  meta: LocalMeta;
}

export interface DownloadsState {
  /** The server answered the last request. */
  online: boolean;
  syncing: boolean;
  paused: boolean;
  /** Unix ms. */
  lastSyncAt: number | null;
  lastError: string | null;
  folder: string;
  usedBytes: number;
  quotaBytes: number | null;
  /** In the server's priority order. */
  items: LocalItem[];
}

export interface LocalPlayback {
  url: string;
  subtitles: LocalSubtitle[];
  durationMs: number | null;
  positionMs: number;
  audioLanguage: string | null;
  /** The next downloaded episode of the same series, for "next episode". */
  nextEpisodeId: number | null;
  meta: LocalMeta;
}

export interface ProgressReport {
  kind: ItemKind;
  id: number;
  positionMs: number;
  played: boolean;
}

export interface DesktopSettings {
  folder: string;
  launchAtLogin: boolean;
}

/** A newer version of the desktop app. */
export type UpdateState =
  | { status: "none" }
  /** Windows, Linux: downloading in the background. */
  | { status: "downloading"; version: string; percent: number }
  /** Windows, Linux: downloaded, installed on restart or when the app quits. */
  | { status: "ready"; version: string }
  /** macOS (unsigned builds can't update themselves): download it from the release page. */
  | { status: "available"; version: string; url: string };

export interface DesktopBridge {
  info(): Promise<DesktopInfo>;
  /** Checks that a batvision server answers there, then remembers it. */
  setServer(url: string): Promise<SetServerResult>;
  /** Forgets the server (only while signed out). */
  forgetServer(): Promise<void>;

  downloads(): Promise<DownloadsState>;
  /** Called on every change; returns an unsubscribe function. */
  onDownloads(listener: (state: DownloadsState) => void): () => void;
  syncNow(): Promise<void>;
  setPaused(paused: boolean): Promise<void>;
  /** Drops an item, downloaded or not (D74): the server forgets what asks for it, the
   * transfer stops and the files go. Rejects when the server can't be reached. */
  cancel(kind: ItemKind, id: number): Promise<void>;
  /** Hardware AV1 decoding, detected by the renderer. */
  setAv1(av1: boolean): Promise<void>;

  /** The downloaded copy of an item, if complete. */
  localPlayback(kind: ItemKind, id: number): Promise<LocalPlayback | null>;
  /** Progress on a downloaded copy; sent to the server now or when it's back. */
  reportProgress(report: ProgressReport): Promise<void>;

  settings(): Promise<DesktopSettings>;
  setLaunchAtLogin(enabled: boolean): Promise<DesktopSettings>;
  /** Opens a folder picker and moves the downloads there. */
  chooseFolder(): Promise<DesktopSettings>;
  openFolder(): Promise<void>;

  update(): Promise<UpdateState>;
  /** Called on every change; returns an unsubscribe function. */
  onUpdate(listener: (state: UpdateState) => void): () => void;
  /** `ready`: restarts into the new version. `available`: opens its release page. */
  installUpdate(): Promise<void>;
}

declare global {
  interface Window {
    batvision?: DesktopBridge;
  }
}
