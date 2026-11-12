/** `a` is a newer version than `b` (`1.10.0` > `1.9.2`; a leading `v` is ignored). */
export function isNewer(a: string, b: string) {
  const parts = (v: string) =>
    (v.replace(/^v/, "").split("-")[0] ?? "").split(".").map((n) => Number.parseInt(n, 10) || 0);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d > 0;
  }
  return false;
}
