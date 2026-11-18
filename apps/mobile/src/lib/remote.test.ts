import { describe, expect, it } from "@jest/globals";
import { nextScrubMs, remoteAction, scrubStepMs } from "./remote";

const hidden = { controls: false, onSeekBar: false };
const shown = { controls: true, onSeekBar: false };
const onBar = { controls: true, onSeekBar: true };

describe("remoteAction", () => {
  it("plays and pauses with OK while the controls are hidden", () => {
    expect(remoteAction("select", hidden)).toEqual({ type: "toggle" });
  });

  it("leaves OK to the focused button once the controls show", () => {
    expect(remoteAction("select", shown)).toEqual({ type: "none" });
    expect(remoteAction("select", onBar)).toEqual({ type: "none" });
  });

  it("seeks with left and right while the controls are hidden", () => {
    expect(remoteAction("left", hidden)).toEqual({ type: "scrub", steps: -1 });
    expect(remoteAction("right", hidden)).toEqual({ type: "scrub", steps: 1 });
  });

  it("moves between buttons with the arrows, except on the seek bar", () => {
    expect(remoteAction("left", shown)).toEqual({ type: "none" });
    expect(remoteAction("right", shown)).toEqual({ type: "none" });
    expect(remoteAction("up", shown)).toEqual({ type: "none" });
    expect(remoteAction("left", onBar)).toEqual({ type: "scrub", steps: -1 });
    expect(remoteAction("right", onBar)).toEqual({ type: "scrub", steps: 1 });
  });

  it("shows the controls with up, down, info and menu", () => {
    for (const key of ["up", "down", "info", "menu"])
      expect(remoteAction(key, hidden)).toEqual({ type: "show" });
  });

  it("obeys media keys whatever is shown", () => {
    for (const state of [hidden, shown, onBar]) {
      expect(remoteAction("playPause", state)).toEqual({ type: "toggle" });
      expect(remoteAction("play", state)).toEqual({ type: "play" });
      expect(remoteAction("pause", state)).toEqual({ type: "pause" });
      expect(remoteAction("rewind", state)).toEqual({ type: "scrub", steps: -1 });
      expect(remoteAction("fastForward", state)).toEqual({ type: "scrub", steps: 1 });
    }
  });

  it("ignores other keys", () => {
    expect(remoteAction("channelUp", hidden)).toEqual({ type: "none" });
  });
});

describe("scrubStepMs", () => {
  it("goes further while presses follow each other quickly", () => {
    expect(scrubStepMs(0)).toBe(10_000);
    expect(scrubStepMs(4)).toBe(10_000);
    expect(scrubStepMs(5)).toBe(30_000);
    expect(scrubStepMs(15)).toBe(60_000);
  });
});

describe("nextScrubMs", () => {
  it("stays inside the video", () => {
    expect(nextScrubMs(5_000, -10_000, 60_000)).toBe(0);
    expect(nextScrubMs(55_000, 10_000, 60_000)).toBe(60_000);
    expect(nextScrubMs(20_000, 10_000, 60_000)).toBe(30_000);
  });

  it("has no end while the duration is unknown", () => {
    expect(nextScrubMs(20_000, 10_000, 0)).toBe(30_000);
  });
});
