/**
 * What the device does with the server's expected set: pure, so it can be tested
 * without a server or a disk.
 */

export interface PlanSubtitle {
  url: string;
  language?: string | null;
  kind: string;
}

/** The fields of the server's `PlanItem` the device uses. */
export interface PlanEntry {
  kind: "movie" | "episode";
  id: number;
  profile: string;
  audio_language?: string | null;
  watched: boolean;
  keep_until?: number | null;
  status: string;
  variant_id?: number | null;
  progress: number;
  size_bytes?: number | null;
  url?: string | null;
  subtitles: PlanSubtitle[];
  title: string;
  episode_title?: string | null;
  series_id?: number | null;
  season_number?: number | null;
  episode_number?: number | null;
  poster_path?: string | null;
  duration_ms?: number | null;
  error?: string | null;
}

/** What is on disk for an item. */
export interface LocalRecord {
  key: string;
  complete: boolean;
  /** The variant the file (or partial file) comes from. */
  variantId: number | null;
  profile: string;
  audioLanguage: string | null;
  /** Bytes on disk. */
  sizeBytes: number;
}

export type Action =
  | { type: "request"; key: string; kind: PlanEntry["kind"]; id: number }
  | { type: "download"; key: string; entry: PlanEntry }
  | { type: "confirm"; key: string; variantId: number };

export interface Decision {
  /** Requests and confirmations to send, then downloads in priority order. */
  actions: Action[];
  /** Local items to delete (no longer expected, over quota, or replaced). */
  remove: string[];
  /** Expected items that don't fit in the quota. */
  overQuota: Set<string>;
  /** Space the kept items take (or will take once downloaded). */
  usedBytes: number;
}

export const itemKey = (kind: string, id: number) => `${kind}-${id}`;

/** A complete local copy that still matches what the server expects. */
function matches(local: LocalRecord, entry: PlanEntry) {
  return local.profile === entry.profile && local.audioLanguage === (entry.audio_language ?? null);
}

/**
 * The server lists items in priority order: keep the longest prefix that fits the
 * quota, delete the rest. An item not encoded yet has no known size: it's requested
 * only while there is room left.
 */
export function decide(
  plan: readonly PlanEntry[],
  local: ReadonlyMap<string, LocalRecord>,
  quotaBytes: number | null,
): Decision {
  const quota = quotaBytes ?? Number.POSITIVE_INFINITY;
  const requests: Action[] = [];
  const downloads: Action[] = [];
  const kept = new Set<string>();
  const overQuota = new Set<string>();
  let used = 0;
  let full = false;

  for (const entry of plan) {
    const key = itemKey(entry.kind, entry.id);
    const have = local.get(key);
    const complete = have?.complete && matches(have, entry) ? have : undefined;

    const size = complete ? complete.sizeBytes : (entry.size_bytes ?? null);
    if (full || (size !== null && used + size > quota)) {
      full = true;
      overQuota.add(key);
      continue;
    }

    if (complete) {
      kept.add(key);
      used += complete.sizeBytes;
      // The server forgot we have it (or the confirmation got lost).
      if (
        entry.status === "ready" &&
        complete.variantId !== null &&
        entry.variant_id === complete.variantId
      ) {
        requests.push({ type: "confirm", key, variantId: complete.variantId });
      }
      continue;
    }

    switch (entry.status) {
      case "ready":
        if (entry.url && size !== null) {
          kept.add(key);
          used += size;
          downloads.push({ type: "download", key, entry });
        }
        break;
      case "queued":
      case "encoding":
        if (used < quota) kept.add(key);
        break;
      case "not_requested":
      // Confirmed earlier but missing here (deleted by hand, or another profile).
      case "downloaded":
        if (used < quota) {
          requests.push({ type: "request", key, kind: entry.kind, id: entry.id });
          kept.add(key);
        }
        break;
      default:
        // failed or unavailable: nothing to do until the server changes its mind.
        break;
    }
  }

  const remove = [...local.values()]
    .filter((l) => {
      if (!kept.has(l.key)) return true;
      const entry = plan.find((e) => itemKey(e.kind, e.id) === l.key);
      if (!entry) return true;
      // A stale copy (other profile or audio) or a partial file of another variant.
      return l.complete ? !matches(l, entry) : l.variantId !== (entry.variant_id ?? null);
    })
    .map((l) => l.key);

  return { actions: [...requests, ...downloads], remove, overQuota, usedBytes: used };
}
