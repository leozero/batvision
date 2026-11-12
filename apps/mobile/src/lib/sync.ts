import { useSyncExternalStore } from "react";
import { BatvisionSync, type SyncItem, type SyncState } from "../../modules/batvision-sync";

export { BatvisionSync, type SyncItem, type SyncState };

// One subscription to the engine, shared by every screen.
let state: SyncState = BatvisionSync.getState();
const listeners = new Set<() => void>();
let subscription: { remove(): void } | null = null;

function subscribe(listener: () => void) {
  listeners.add(listener);
  if (!subscription) {
    subscription = BatvisionSync.addListener("onState", (next) => {
      state = next;
      for (const l of listeners) l();
    });
    state = BatvisionSync.getState();
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && subscription) {
      subscription.remove();
      subscription = null;
    }
  };
}

/** Live state of the download engine. */
export function useSyncState(): SyncState {
  return useSyncExternalStore(subscribe, () => state);
}

/** This phone's copy of an item, from the live state. */
export function useLocalItem(kind: "movie" | "episode", id: number): SyncItem | undefined {
  return useSyncState().items.find((i) => i.kind === kind && i.id === id);
}
