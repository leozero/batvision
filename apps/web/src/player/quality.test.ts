import { beforeEach, describe, expect, it } from "vitest";
import {
  type AutoInput,
  autoStep,
  ladder,
  type QualityOption,
  readAutoLevel,
  readQualityMode,
  saveAutoLevel,
  saveQualityMode,
} from "./quality";

const options: QualityOption[] = [
  { quality: "max", height: 2160, bitrate_kbps: 40_000, original: true },
  { quality: "medium", height: 1080, bitrate_kbps: 6_500, original: false },
  { quality: "low", height: 720, bitrate_kbps: 2_128, original: false },
  { quality: "sos", height: 480, bitrate_kbps: 1_096, original: false },
];

const NOW = 10_000_000;

const input = (over: Partial<AutoInput>): AutoInput => ({
  current: "max",
  options,
  estimateKbps: null,
  stalls: [],
  stallingSince: null,
  loadingSince: null,
  now: NOW,
  stableSince: NOW - 10_000,
  lastUpAt: 0,
  ...over,
});

describe("ladder", () => {
  it("drops levels that don't change the stream", () => {
    const light: QualityOption[] = [
      { quality: "max", height: 720, bitrate_kbps: 1_800, original: true },
      { quality: "medium", height: 720, bitrate_kbps: 1_800, original: true },
      { quality: "low", height: 720, bitrate_kbps: 1_800, original: true },
      { quality: "sos", height: 480, bitrate_kbps: 1_096, original: false },
    ];
    expect(ladder(light).map((o) => o.quality)).toEqual(["max", "sos"]);
  });
});

describe("autoStep", () => {
  it("stays put when nothing is known", () => {
    expect(autoStep(input({}))).toBeNull();
  });

  it("goes one level down after two stalls in a minute, or a long one", () => {
    expect(autoStep(input({ stalls: [NOW - 50_000, NOW - 1_000] }))).toBe("medium");
    expect(autoStep(input({ stalls: [NOW - 90_000, NOW - 1_000] }))).toBeNull();
    expect(autoStep(input({ current: "medium", stallingSince: NOW - 5_000 }))).toBe("low");
    expect(autoStep(input({ current: "sos", stallingSince: NOW - 5_000 }))).toBeNull();
  });

  it("goes one level down when a stream shows nothing for 10 s", () => {
    expect(autoStep(input({ loadingSince: NOW - 6_000 }))).toBeNull();
    expect(autoStep(input({ loadingSince: NOW - 11_000 }))).toBe("medium");
  });

  it("jumps to the level the measured throughput carries", () => {
    expect(autoStep(input({ estimateKbps: 12_000 }))).toBe("medium");
    expect(autoStep(input({ estimateKbps: 2_000 }))).toBe("sos");
    expect(autoStep(input({ estimateKbps: 500 }))).toBe("sos");
    expect(autoStep(input({ current: "medium", estimateKbps: 10_000 }))).toBeNull();
  });

  it("goes up slowly, after stable playback with room to spare", () => {
    const ready = {
      current: "low" as const,
      estimateKbps: 14_000,
      stableSince: NOW - 130_000,
      lastUpAt: NOW - 400_000,
    };
    expect(autoStep(input(ready))).toBe("medium");
    expect(autoStep(input({ ...ready, stableSince: NOW - 60_000 }))).toBeNull();
    expect(autoStep(input({ ...ready, lastUpAt: NOW - 100_000 }))).toBeNull();
    expect(autoStep(input({ ...ready, estimateKbps: 12_000 }))).toBeNull();
    expect(autoStep(input({ ...ready, current: "medium", estimateKbps: 90_000 }))).toBe("max");
    expect(autoStep(input({ ...ready, current: "sos", estimateKbps: 90_000 }))).toBe("max");
  });

  it("treats a level equal to the one above as that one", () => {
    const light: QualityOption[] = [
      { quality: "max", height: 720, bitrate_kbps: 1_800, original: true },
      { quality: "medium", height: 720, bitrate_kbps: 1_800, original: true },
      { quality: "low", height: 720, bitrate_kbps: 1_800, original: true },
      { quality: "sos", height: 480, bitrate_kbps: 1_096, original: false },
    ];
    expect(autoStep(input({ current: "medium", options: light, stallingSince: NOW - 5_000 }))).toBe(
      "sos",
    );
  });
});

describe("stored choice", () => {
  beforeEach(() => localStorage.clear());

  it("defaults to auto, starting at max", () => {
    expect(readQualityMode()).toBe("auto");
    expect(readAutoLevel()).toBe("max");
  });

  it("is remembered on the device", () => {
    saveQualityMode("low");
    saveAutoLevel("sos");
    expect(readQualityMode()).toBe("low");
    expect(readAutoLevel()).toBe("sos");
    localStorage.setItem("bv.quality", "nonsense");
    expect(readQualityMode()).toBe("auto");
  });
});
