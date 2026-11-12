import type { Schemas } from "../lib/api";

/** Picture quality levels (D60), best first. */
export type Quality = Schemas["Quality"];
export type QualityOption = Schemas["QualityOption"];
/** What the user picked: a fixed level, or automatic. */
export type QualityMode = "auto" | Quality;

export const LEVELS: Quality[] = ["max", "medium", "low", "sos"];

const MODE_KEY = "bv.quality";
const AUTO_KEY = "bv.quality.auto";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // ignore
  }
}

const isLevel = (v: string | null): v is Quality => LEVELS.includes(v as Quality);

/** The choice is kept per device: the network depends on where it is. */
export function readQualityMode(): QualityMode {
  const v = read(MODE_KEY);
  return v === "auto" || isLevel(v) ? v : "auto";
}

export const saveQualityMode = (mode: QualityMode) => write(MODE_KEY, mode);

/** Auto starts from where it settled last time on this device. */
export function readAutoLevel(): Quality {
  const v = read(AUTO_KEY);
  return isLevel(v) ? v : "max";
}

export const saveAutoLevel = (level: Quality) => write(AUTO_KEY, level);

// ---- Automatic choice -------------------------------------------------------------------

/** Below this many times the stream's bitrate, the network is too slow: go down. */
export const DOWN_FACTOR = 1.5;
/** Above this many times the next level's bitrate, try going up. */
export const UP_FACTOR = 2;
/** Playback without stalls needed before going up. */
export const STABLE_BEFORE_UP_MS = 120_000;
/** At most one attempt to go up in this time. */
export const UP_EVERY_MS = 300_000;
/** Two stalls within this window mean the network can't keep up. */
export const STALL_WINDOW_MS = 60_000;
/** A single stall this long is enough. */
export const LONG_STALL_MS = 4_000;
/** A stream that shows nothing for this long is too heavy (or too slow to encode). */
export const SLOW_START_MS = 10_000;

/** Levels that actually change the stream, best first (a source lighter than a level
 *  plays the same at that level and the one above). */
export function ladder(options: QualityOption[]): QualityOption[] {
  const out: QualityOption[] = [];
  for (const option of options) {
    const previous = out[out.length - 1];
    if (
      previous &&
      previous.height === option.height &&
      previous.bitrate_kbps === option.bitrate_kbps &&
      previous.original === option.original
    )
      continue;
    out.push(option);
  }
  return out;
}

export type AutoInput = {
  current: Quality;
  options: QualityOption[];
  /** Measured network throughput, kbit/s; `null` when unknown (direct play, too few samples). */
  estimateKbps: number | null;
  /** Start times of the stalls since this stream began. */
  stalls: number[];
  /** Start of the stall in progress, if any. */
  stallingSince: number | null;
  /** When this stream was asked for, while it hasn't played yet. */
  loadingSince: number | null;
  now: number;
  /** Start of this stream or of the last stall. */
  stableSince: number;
  lastUpAt: number;
};

/** The level Auto should switch to, or `null` to stay. Goes down fast, up slowly. */
export function autoStep(input: AutoInput): Quality | null {
  const steps = ladder(input.options);
  const currentOption = input.options.find((o) => o.quality === input.current);
  let index = steps.findIndex(
    (o) =>
      o.height === currentOption?.height &&
      o.bitrate_kbps === currentOption?.bitrate_kbps &&
      o.original === currentOption?.original,
  );
  if (index < 0) index = 0;
  const current = steps[index];
  if (!current) return null;
  const { estimateKbps: estimate, now } = input;

  // Down.
  if (index < steps.length - 1) {
    const recentStalls = input.stalls.filter((t) => now - t <= STALL_WINDOW_MS).length;
    const longStall = input.stallingSince !== null && now - input.stallingSince >= LONG_STALL_MS;
    const slowStart = input.loadingSince !== null && now - input.loadingSince >= SLOW_START_MS;
    if (recentStalls >= 2 || longStall || slowStart) return steps[index + 1]?.quality ?? null;
    const rate = current.bitrate_kbps;
    if (estimate !== null && rate && estimate < rate * DOWN_FACTOR) {
      // Straight to the best level the network carries, or the lowest one.
      const fits = steps
        .slice(index + 1)
        .find((o) => o.bitrate_kbps && estimate >= o.bitrate_kbps * DOWN_FACTOR);
      return (fits ?? steps[steps.length - 1])?.quality ?? null;
    }
  }

  // Up, straight to the best level the network carries with room to spare.
  if (
    index > 0 &&
    estimate !== null &&
    input.stallingSince === null &&
    input.loadingSince === null &&
    now - input.stableSince >= STABLE_BEFORE_UP_MS &&
    now - input.lastUpAt >= UP_EVERY_MS
  ) {
    const best = steps
      .slice(0, index)
      .find((o) => o.bitrate_kbps && estimate >= o.bitrate_kbps * UP_FACTOR);
    if (best) return best.quality;
  }
  return null;
}
