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

/** How someone watches (D104): the languages they understand, best first. */
export interface TrackPreferences {
  /** ISO 639-2; missing in preferences cached by apps before 0.9. */
  spoken_languages?: readonly string[];
  /** For what they don't understand: the dub rather than the original with subtitles. */
  prefer_dub?: boolean;
  subtitle_mode: SubtitleMode;
}

/** The languages offered in the picker, in that order (ISO 639-2). */
export const SPOKEN_LANGUAGE_CHOICES = [
  "fre",
  "eng",
  "ger",
  "spa",
  "ita",
  "por",
  "dut",
  "jpn",
  "kor",
  "chi",
  "rus",
  "pol",
  "ara",
  "tur",
] as const;

/** The understood languages: the saved ones, else the account's. */
export function spokenLanguages(prefs: TrackPreferences | undefined, language: string): string[] {
  const saved = prefs?.spoken_languages ?? [];
  return saved.length ? [...saved] : [language];
}

export interface SubtitleChoice {
  language: string | null;
  /** `normal`, `forced`, `sdh`… */
  kind: string;
}

/**
 * The subtitle a downloaded copy starts with (-1: none), by the same rules as the server:
 * subtitles in the first understood language that has some when the audio isn't
 * understood, else only a "forced" track. Without preferences (offline before they were
 * ever loaded), only a "forced" track.
 */
export function defaultSubtitle(
  subtitles: readonly SubtitleChoice[],
  audioLanguage: string | null | undefined,
  prefs: TrackPreferences | undefined,
  language: string,
): number {
  if (!prefs) return subtitles.findIndex((s) => s.kind === "forced");
  const spoken = spokenLanguages(prefs, language);
  const find = (languages: readonly (string | null | undefined)[], kinds: string[]) => {
    for (const lang of languages) {
      for (const kind of kinds) {
        const i = subtitles.findIndex((s) => s.kind === kind && sameLanguage(s.language, lang));
        if (i >= 0) return i;
      }
    }
    return -1;
  };
  const forced = () => {
    const i = find([audioLanguage], ["forced"]);
    return i >= 0 ? i : find(spoken, ["forced"]);
  };
  const understood = spoken.some((l) => sameLanguage(l, audioLanguage));
  switch (prefs.subtitle_mode) {
    case "never":
      return forced();
    case "always":
      return find(spoken, ["normal", "sdh"]);
    case "foreign": {
      if (understood) return forced();
      const i = find(spoken, ["normal", "sdh"]);
      return i >= 0 ? i : find(spoken, ["forced"]);
    }
  }
}
