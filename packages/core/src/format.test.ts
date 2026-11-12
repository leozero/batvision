import { describe, expect, it } from "vitest";
import { episodeCode, formatClock, formatRuntime, languageName, progressRatio } from "./format";

describe("format", () => {
  it("formats runtimes per locale", () => {
    expect(formatRuntime(5_400_000, "fr")).toBe("1 h 30");
    expect(formatRuntime(5_400_000, "en")).toBe("1h 30m");
    expect(formatRuntime(1_500_000, "fr")).toBe("25 min");
    expect(formatRuntime(null, "fr")).toBeNull();
  });

  it("formats player clocks", () => {
    expect(formatClock(3723)).toBe("1:02:03");
    expect(formatClock(123)).toBe("2:03");
    expect(formatClock(-5)).toBe("0:00");
  });

  it("names languages from ISO 639-2 codes", () => {
    expect(languageName("fre", "fr")).toBe("Français");
    expect(languageName("eng", "en")).toBe("English");
    expect(languageName(null, "fr")).toBeNull();
  });

  it("codes episodes and computes progress", () => {
    expect(episodeCode(1, 2)).toBe("S1 · E2");
    expect(episodeCode(0, 3)).toBe("SP · E3");
    expect(progressRatio(30, 120)).toBe(0.25);
    expect(progressRatio(10, null)).toBe(0);
    expect(progressRatio(500, 100)).toBe(1);
  });
});
