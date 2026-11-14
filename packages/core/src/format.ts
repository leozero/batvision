/** 5400000 → "1 h 30" / "1h 30m" depending on the locale. */
export function formatRuntime(ms: number | null | undefined, lang: string): string | null {
  if (!ms || ms <= 0) return null;
  const minutes = Math.round(ms / 60000);
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (lang.startsWith("fr")) return h ? `${h} h ${String(m).padStart(2, "0")}` : `${m} min`;
  return h ? `${h}h ${m}m` : `${m}m`;
}

/** Seconds → "1:02:03" or "2:03". */
export function formatClock(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

/** ISO 639-2 code → language name in the UI language ("fre" → "Français"). */
const iso6392to1: Record<string, string> = {
  fre: "fr",
  eng: "en",
  ger: "de",
  spa: "es",
  ita: "it",
  por: "pt",
  dut: "nl",
  jpn: "ja",
  kor: "ko",
  chi: "zh",
  rus: "ru",
  pol: "pl",
  swe: "sv",
  dan: "da",
  nor: "no",
  fin: "fi",
  cze: "cs",
  gre: "el",
  ara: "ar",
  heb: "he",
  tur: "tr",
  hun: "hu",
  rum: "ro",
  ukr: "uk",
  hin: "hi",
};

/** ISO 639-2/T codes players report where files say ISO 639-2/B ("fra" for "fre"). */
const iso6392tTo1: Record<string, string> = {
  fra: "fr",
  deu: "de",
  nld: "nl",
  zho: "zh",
  ces: "cs",
  ell: "el",
  ron: "ro",
};

/** "fre", "fra", "fr-FR" → "fr"; other codes as they are, lowercased. */
function languageCode(code: string): string {
  const c = code.toLowerCase().split(/[-_]/)[0] ?? "";
  return iso6392to1[c] ?? iso6392tTo1[c] ?? c;
}

/** "fre", "fra", "fr" and "fr-FR" are the same language, whatever a player reports. */
export function sameLanguage(a: string | null | undefined, b: string | null | undefined): boolean {
  const left = a ? languageCode(a) : null;
  return left !== null && left !== "und" && left === (b ? languageCode(b) : null);
}

/** For engines without `Intl.DisplayNames` (Hermes, in the mobile app). */
const names: Record<string, [fr: string, en: string]> = {
  fr: ["Français", "French"],
  en: ["Anglais", "English"],
  de: ["Allemand", "German"],
  es: ["Espagnol", "Spanish"],
  it: ["Italien", "Italian"],
  pt: ["Portugais", "Portuguese"],
  nl: ["Néerlandais", "Dutch"],
  ja: ["Japonais", "Japanese"],
  ko: ["Coréen", "Korean"],
  zh: ["Chinois", "Chinese"],
  ru: ["Russe", "Russian"],
  pl: ["Polonais", "Polish"],
  sv: ["Suédois", "Swedish"],
  da: ["Danois", "Danish"],
  no: ["Norvégien", "Norwegian"],
  fi: ["Finnois", "Finnish"],
  cs: ["Tchèque", "Czech"],
  el: ["Grec", "Greek"],
  ar: ["Arabe", "Arabic"],
  he: ["Hébreu", "Hebrew"],
  tr: ["Turc", "Turkish"],
  hu: ["Hongrois", "Hungarian"],
  ro: ["Roumain", "Romanian"],
  uk: ["Ukrainien", "Ukrainian"],
  hi: ["Hindi", "Hindi"],
};

export function languageName(code: string | null | undefined, uiLang: string): string | null {
  if (!code) return null;
  const short = languageCode(code);
  try {
    const name = new Intl.DisplayNames([uiLang], { type: "language" }).of(short);
    if (name && name !== short) return name.charAt(0).toUpperCase() + name.slice(1);
  } catch {
    // No Intl.DisplayNames: the table below.
  }
  const known = names[short];
  return known ? known[uiLang.startsWith("fr") ? 0 : 1] : code;
}

/** "S1 · E2" */
export function episodeCode(season: number | null | undefined, episode: number | null | undefined) {
  if (season == null || episode == null) return "";
  return season === 0 ? `SP · E${episode}` : `S${season} · E${episode}`;
}

export function progressRatio(positionMs: number, durationMs: number | null | undefined) {
  if (!durationMs || positionMs <= 0) return 0;
  return Math.min(1, positionMs / durationMs);
}
