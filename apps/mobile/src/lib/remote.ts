/**
 * What the TV remote does in the player (D112). Pure, so it's tested without a TV.
 *
 * Controls hidden: OK pauses or plays, left and right seek (the controls show with the
 * seek bar, its picture following), the other arrows show the controls. Controls shown:
 * the arrows move between buttons, except left and right on the seek bar, which seek.
 * Media keys work everywhere.
 */
export type RemoteAction =
  | { type: "toggle" }
  | { type: "play" }
  | { type: "pause" }
  /** Move the seek preview by this many steps (negative: back). */
  | { type: "scrub"; steps: number }
  /** Show the controls, the remote on play/pause. */
  | { type: "show" }
  | { type: "none" };

export function remoteAction(
  key: string,
  state: { controls: boolean; onSeekBar: boolean },
): RemoteAction {
  switch (key) {
    case "playPause":
      return { type: "toggle" };
    case "play":
      return { type: "play" };
    case "pause":
      return { type: "pause" };
    case "rewind":
      return { type: "scrub", steps: -1 };
    case "fastForward":
      return { type: "scrub", steps: 1 };
    case "left":
    case "right":
      if (state.controls && !state.onSeekBar) return { type: "none" };
      return { type: "scrub", steps: key === "left" ? -1 : 1 };
    case "select":
      return state.controls ? { type: "none" } : { type: "toggle" };
    case "up":
    case "down":
    case "info":
    case "menu":
      return state.controls ? { type: "none" } : { type: "show" };
    default:
      return { type: "none" };
  }
}

/** Presses in quick succession: each step goes further (10 s, then 30 s, then a minute). */
export function scrubStepMs(repeats: number): number {
  if (repeats < 5) return 10_000;
  if (repeats < 15) return 30_000;
  return 60_000;
}

/** Presses closer than this make one run. */
export const REPEAT_WINDOW_MS = 400;

/** The preview's new time (ms), kept inside the video. */
export function nextScrubMs(from: number, deltaMs: number, durationMs: number): number {
  const target = from + deltaMs;
  return Math.max(0, durationMs > 0 ? Math.min(target, durationMs) : target);
}
