import { sameLanguage } from "./format";

/** ISO 639-1 → the ISO 639-2/B codes the server stores (`fr` → `fre`). */
const toIso6392: Record<string, string> = {
  fr: "fre",
  en: "eng",
  de: "ger",
  es: "spa",
  it: "ita",
  pt: "por",
  nl: "dut",
  ja: "jpn",
  ko: "kor",
  zh: "chi",
  ru: "rus",
  pl: "pol",
};

/** The account's UI language (`fr`, `en-GB`…) as a track language (`fre`, `eng`). */
export function trackLanguage(uiLanguage: string): string {
  const base = uiLanguage.toLowerCase().split(/[-_]/)[0] ?? "";
  return toIso6392[base] ?? base;
}

export type SubtitleMode = "always" | "foreign" | "never";

export interface TrackPreferences {
  audio_language?: string | null;
  subtitle_language?: string | null;
  subtitle_mode: SubtitleMode;
}

/**
 * How someone likes to watch (D103): dubbed in their language, original version with
 * their subtitles, or original version alone. `custom` is anything else set by hand.
 */
export type LanguagePreset = "dubbed" | "subtitled" | "original" | "custom";

export const LANGUAGE_PRESETS: readonly Exclude<LanguagePreset, "custom">[] = [
  "dubbed",
  "subtitled",
  "original",
];

/** The server's value for "the language the movie was made in". */
export const ORIGINAL_AUDIO = "original";

/** The preferences a preset stands for, for someone whose language is `language`. */
export function presetPreferences(
  preset: Exclude<LanguagePreset, "custom">,
  language: string,
): TrackPreferences {
  switch (preset) {
    case "dubbed":
      // Their language when the file has it; subtitles only if it doesn't.
      return { audio_language: language, subtitle_language: language, subtitle_mode: "foreign" };
    case "subtitled":
      // Subtitles whenever the original isn't in their language.
      return {
        audio_language: ORIGINAL_AUDIO,
        subtitle_language: language,
        subtitle_mode: "foreign",
      };
    case "original":
      return {
        audio_language: ORIGINAL_AUDIO,
        subtitle_language: language,
        subtitle_mode: "never",
      };
  }
}

/** Which preset saved preferences match, or `custom`. */
export function presetOf(prefs: TrackPreferences, language: string): LanguagePreset {
  if (prefs.audio_language === ORIGINAL_AUDIO) {
    if (prefs.subtitle_mode === "never") return "original";
    if (prefs.subtitle_mode === "foreign" && sameLanguage(prefs.subtitle_language, language))
      return "subtitled";
    return "custom";
  }
  if (
    sameLanguage(prefs.audio_language, language) &&
    prefs.subtitle_mode === "foreign" &&
    (prefs.subtitle_language == null || sameLanguage(prefs.subtitle_language, language))
  )
    return "dubbed";
  return "custom";
}

export interface SubtitleChoice {
  language: string | null;
  /** `normal`, `forced`, `sdh`… */
  kind: string;
}

/**
 * The subtitle a downloaded copy starts with (-1: none), by the same rules as the server:
 * without preferences (offline before they were ever loaded), only a "forced" track.
 */
export function defaultSubtitle(
  subtitles: readonly SubtitleChoice[],
  audioLanguage: string | null | undefined,
  prefs: TrackPreferences | undefined,
  language: string,
): number {
  const wanted = prefs?.subtitle_language ?? language;
  const find = (kinds: string[]) => {
    for (const kind of kinds) {
      const i = subtitles.findIndex((s) => s.kind === kind && sameLanguage(s.language, wanted));
      if (i >= 0) return i;
    }
    return -1;
  };
  if (!prefs) return subtitles.findIndex((s) => s.kind === "forced");
  switch (prefs.subtitle_mode) {
    case "never":
      return find(["forced"]);
    case "always":
      return find(["normal", "sdh"]);
    case "foreign":
      return sameLanguage(audioLanguage, wanted)
        ? find(["forced"])
        : find(["normal", "sdh", "forced"]);
  }
}
