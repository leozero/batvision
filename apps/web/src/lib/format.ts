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

export function languageName(code: string | null | undefined, uiLang: string): string | null {
  if (!code) return null;
  const short = iso6392to1[code] ?? code;
  try {
    const name = new Intl.DisplayNames([uiLang], { type: "language" }).of(short);
    return name ? name.charAt(0).toUpperCase() + name.slice(1) : code;
  } catch {
    return code;
  }
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
