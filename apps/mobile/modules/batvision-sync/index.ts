import type { Schemas } from "@batvision/api-client";
import type { ItemKind, LocalItem, LocalPlayback } from "@batvision/bridge";
import { type EventSubscription, NativeModule, requireNativeModule } from "expo";

/** Why an item waits on this phone (D68, D69). */
export type Wait = "offline" | "wifi" | "charging" | "mobile_cap";

export type SyncItem = LocalItem & { wait: Wait | null };

/** The Kotlin engine's state (D63). Image and video URLs are `file://` once downloaded. */
export interface SyncState {
  signedIn: boolean;
  online: boolean;
  syncing: boolean;
  paused: boolean;
  lastSyncAt: number | null;
  lastError: string | null;
  storage: "internal" | "sd";
  sdAvailable: boolean;
  usedBytes: number;
  quotaBytes: number | null;
  mobileBytesThisMonth: number;
  mobileCapBytes: number | null;
  items: SyncItem[];
}

type Events = { onState(state: SyncState): void };

declare class BatvisionSyncModule extends NativeModule<Events> {
  getSession(): { server: string | null; token: string | null; deviceId: number | null };
  setServer(server: string | null): void;
  signIn(token: string, deviceId: number): void;
  signOut(): void;
  getState(): SyncState;
  syncNow(): void;
  setPaused(paused: boolean): void;
  localPlayback(kind: ItemKind, id: number): LocalPlayback | null;
  reportProgress(kind: ItemKind, id: number, positionMs: number, played: boolean): void;
  deviceInfo(): { model: string; manufacturer: string; av1: boolean };
  /** What this phone decodes, for the server's playback decision. */
  capabilities(): Schemas["Capabilities"];
}

/** The download engine and the session it uses (Kotlin, `android/`). */
export const BatvisionSync = requireNativeModule<BatvisionSyncModule>("BatvisionSync");

export type { EventSubscription };
