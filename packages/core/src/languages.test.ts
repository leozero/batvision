import { describe, expect, it } from "vitest";
import { defaultSubtitle, spokenLanguages, trackLanguage } from "./languages";

describe("spoken languages", () => {
  it("maps the UI language to a track language", () => {
    expect(trackLanguage("fr")).toBe("fre");
    expect(trackLanguage("en-GB")).toBe("eng");
  });

  it("falls back on the account's language", () => {
    expect(
      spokenLanguages({ spoken_languages: ["eng", "fre"], subtitle_mode: "foreign" }, "fre"),
    ).toEqual(["eng", "fre"]);
    expect(spokenLanguages({ subtitle_mode: "foreign" }, "fre")).toEqual(["fre"]);
    expect(spokenLanguages(undefined, "eng")).toEqual(["eng"]);
  });
});

describe("defaultSubtitle", () => {
  const subs = [
    { language: "eng", kind: "normal" },
    { language: "fre", kind: "forced" },
    { language: "fre", kind: "normal" },
  ];
  const prefs = (mode: "always" | "foreign" | "never", spoken = ["fre"]) => ({
    spoken_languages: spoken,
    prefer_dub: false,
    subtitle_mode: mode,
  });

  it("never starts on the first track just because it's first", () => {
    expect(defaultSubtitle(subs, "eng", undefined, "fre")).toBe(1);
    expect(defaultSubtitle([{ language: "eng", kind: "normal" }], "eng", undefined, "fre")).toBe(
      -1,
    );
  });

  it("shows subtitles when the audio isn't understood", () => {
    expect(defaultSubtitle(subs, "eng", prefs("foreign"), "fre")).toBe(2);
    expect(defaultSubtitle(subs, "fre", prefs("foreign"), "fre")).toBe(1);
    expect(defaultSubtitle(subs, "eng", prefs("foreign", ["eng", "fre"]), "fre")).toBe(1);
    // Their second language when the first has none.
    expect(defaultSubtitle(subs, "jpn", prefs("foreign", ["ita", "eng"]), "fre")).toBe(0);
  });

  it("follows the subtitle mode", () => {
    expect(defaultSubtitle(subs, "eng", prefs("never"), "fre")).toBe(1);
    expect(defaultSubtitle(subs, "fre", prefs("always"), "fre")).toBe(2);
  });

  it("reads preferences cached by older apps", () => {
    expect(defaultSubtitle(subs, "eng", { subtitle_mode: "foreign" }, "fre")).toBe(2);
  });
});
