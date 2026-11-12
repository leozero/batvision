/** 1536000000 → "1,5 Go" / "1.5 GB". */
export function formatBytes(bytes: number | null | undefined, lang: string): string {
  if (bytes == null) return "—";
  const fr = lang.startsWith("fr");
  const units = fr ? ["o", "Ko", "Mo", "Go", "To"] : ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit++;
  }
  const digits = value < 10 && unit > 0 ? 1 : 0;
  return `${value.toLocaleString(lang, { maximumFractionDigits: digits })} ${units[unit]}`;
}
