/** A subtitle cue: shown from `start` to `end`, in seconds. */
export interface Cue {
  start: number;
  end: number;
  text: string;
}

/** "01:02:03.456" or "02:03.456" → seconds. */
function seconds(stamp: string): number | null {
  const m = /^(?:(\d+):)?(\d{1,2}):(\d{2})[.,](\d{1,3})$/.exec(stamp.trim());
  if (!m) return null;
  return (
    Number(m[1] ?? 0) * 3600 +
    Number(m[2]) * 60 +
    Number(m[3]) +
    Number((m[4] ?? "0").padEnd(3, "0")) / 1000
  );
}

/**
 * The cues of a WebVTT file, for players that draw subtitles themselves (iOS downloads:
 * side-loaded files would drop every audio track but the first, D40). Tags are dropped.
 */
export function parseWebVtt(vtt: string): Cue[] {
  const cues: Cue[] = [];
  for (const block of vtt.replace(/\r\n?/g, "\n").split(/\n{2,}/)) {
    const lines = block.split("\n");
    const at = lines.findIndex((l) => l.includes("-->"));
    if (at < 0) continue;
    const [from = "", rest = ""] = (lines[at] ?? "").split("-->");
    const start = seconds(from);
    const end = seconds(rest.trim().split(/\s+/)[0] ?? "");
    const text = lines
      .slice(at + 1)
      .join("\n")
      .replace(/<[^>]+>/g, "")
      .replace(/&amp;/g, "&")
      .replace(/&lt;/g, "<")
      .replace(/&gt;/g, ">")
      .replace(/&nbsp;/g, " ")
      .trim();
    if (start !== null && end !== null && text) cues.push({ start, end, text });
  }
  return cues.sort((a, b) => a.start - b.start);
}

/** The text shown at `time` (seconds): overlapping cues stack, earliest first. */
export function cueAt(cues: readonly Cue[], time: number): string | null {
  const shown = cues.filter((c) => c.start <= time && time < c.end).map((c) => c.text);
  return shown.length ? shown.join("\n") : null;
}
