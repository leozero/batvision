import { describe, expect, it } from "vitest";
import { defaultSubtitle, presetOf, presetPreferences, trackLanguage } from "./languages";

describe("language presets", () => {
  it("maps the UI language to a track language", () => {
    expect(trackLanguage("fr")).toBe("fre");
    expect(trackLanguage("en-GB")).toBe("eng");
  });

  it("round-trips each preset", () => {
    for (const p of ["dubbed", "subtitled", "original"] as const) {
      expect(presetOf(presetPreferences(p, "fre"), "fre")).toBe(p);
    }
    expect(
      presetOf({ audio_language: "eng", subtitle_language: "fre", subtitle_mode: "always" }, "fre"),
    ).toBe("custom");
  });
});

describe("defaultSubtitle", () => {
  const subs = [
    { language: "eng", kind: "normal" },
    { language: "fre", kind: "forced" },
    { language: "fre", kind: "normal" },
  ];
  const prefs = (mode: "always" | "foreign" | "never") => ({
    audio_language: "original",
    subtitle_language: "fre",
    subtitle_mode: mode,
  });

  it("never starts on the first track just because it's first", () => {
    expect(defaultSubtitle(subs, "eng", undefined, "fre")).toBe(1);
    expect(defaultSubtitle([{ language: "eng", kind: "normal" }], "eng", undefined, "fre")).toBe(
      -1,
    );
  });

  it("follows the subtitle mode", () => {
    expect(defaultSubtitle(subs, "eng", prefs("foreign"), "fre")).toBe(2);
    expect(defaultSubtitle(subs, "fre", prefs("foreign"), "fre")).toBe(1);
    expect(defaultSubtitle(subs, "eng", prefs("never"), "fre")).toBe(1);
    expect(defaultSubtitle(subs, "fre", prefs("always"), "fre")).toBe(2);
  });
});
