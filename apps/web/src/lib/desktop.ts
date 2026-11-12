import type {
  DesktopBridge,
  DesktopInfo,
  DownloadsState,
  ItemKind,
  LocalItem,
} from "@batvision/bridge";
import { queryOptions } from "@tanstack/react-query";
import { useSyncExternalStore } from "react";
import { ApiError } from "./api";

export type { DownloadsState, LocalItem };

/** The desktop app's bridge; `undefined` in a browser. */
export const desktop: DesktopBridge | undefined =
  typeof window === "undefined" ? undefined : window.batvision;

let info: DesktopInfo | null = null;

/** Loads what the desktop app knows (server, device). Call before rendering. */
export async function loadDesktopInfo() {
  if (desktop) info = await desktop.info();
  return info;
}

export function desktopInfo() {
  return info;
}

/** The desktop app has a server address. */
export function hasServer() {
  return !desktop || Boolean(info?.server);
}

/** The server didn't answer (desktop only: the app's proxy says so). */
export function isUnreachable(error: unknown) {
  return error instanceof ApiError && error.status === 503 && error.code === "server_unreachable";
}

// One subscription to the desktop app, shared by every component.
let downloadsState: DownloadsState | undefined;
const listeners = new Set<() => void>();
let unsubscribe: (() => void) | null = null;

function subscribeDownloads(listener: () => void) {
  listeners.add(listener);
  if (desktop && !unsubscribe) {
    const set = (state: DownloadsState) => {
      downloadsState = state;
      for (const l of listeners) l();
    };
    unsubscribe = desktop.onDownloads(set);
    void desktop.downloads().then(set);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && unsubscribe) {
      unsubscribe();
      unsubscribe = null;
    }
  };
}

/** Live downloads state from the desktop app (`undefined` in a browser). */
export function useDownloadsState(): DownloadsState | undefined {
  return useSyncExternalStore(subscribeDownloads, () => downloadsState);
}

/** The local copy of an item, from the live state. */
export function useLocalItem(kind: ItemKind, id: number): LocalItem | undefined {
  const state = useDownloadsState();
  return state?.items.find((i) => i.kind === kind && i.id === id);
}

export const localPlaybackQuery = (kind: ItemKind, id: number) =>
  queryOptions({
    queryKey: ["local-playback", kind, id],
    queryFn: async () => (desktop ? await desktop.localPlayback(kind, id) : null),
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });

/** Hardware AV1 decoding: lighter downloads are then encoded in AV1. */
export async function reportAv1Support() {
  if (!desktop || !navigator.mediaCapabilities) return;
  try {
    const result = await navigator.mediaCapabilities.decodingInfo({
      type: "file",
      video: {
        contentType: 'video/mp4; codecs="av01.0.08M.08"',
        width: 1920,
        height: 1080,
        bitrate: 3_000_000,
        framerate: 24,
      },
    });
    await desktop.setAv1(result.supported && result.powerEfficient);
  } catch {
    // Unknown: the server keeps HEVC.
  }
}

/** Asks the desktop app to sync now (after changing what this device should hold). */
export function syncDesktop() {
  void desktop?.syncNow();
}
