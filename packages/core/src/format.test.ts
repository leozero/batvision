import { describe, expect, it } from "vitest";
import {
  episodeCode,
  formatClock,
  formatRuntime,
  languageName,
  progressRatio,
  sameLanguage,
} from "./format";

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

  it("matches a language whatever code a player reports", () => {
    expect(sameLanguage("fre", "fr")).toBe(true);
    expect(sameLanguage("fra", "fre")).toBe(true);
    expect(sameLanguage("fr-FR", "fre")).toBe(true);
    expect(sameLanguage("eng", "fre")).toBe(false);
    expect(sameLanguage("und", "und")).toBe(false);
    expect(sameLanguage(null, null)).toBe(false);
  });

  it("names languages without Intl.DisplayNames", () => {
    // Hermes has none.
    const intl = Intl as { DisplayNames?: unknown };
    const real = intl.DisplayNames;
    intl.DisplayNames = undefined;
    try {
      expect(languageName("fre", "fr")).toBe("Français");
      expect(languageName("en", "fr")).toBe("Anglais");
      expect(languageName("fra", "en")).toBe("French");
      expect(languageName("xyz", "fr")).toBe("xyz");
    } finally {
      intl.DisplayNames = real;
    }
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
