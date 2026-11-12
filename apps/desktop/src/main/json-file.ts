import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

/** Reads a JSON file, or `fallback` when it's missing or unreadable. */
export function readJson<T>(path: string, fallback: T): T {
  try {
    return { ...fallback, ...JSON.parse(readFileSync(path, "utf8")) };
  } catch {
    return fallback;
  }
}

/** Writes through a temporary file so a crash never leaves half a file. */
export function writeJson(path: string, value: unknown) {
  mkdirSync(dirname(path), { recursive: true });
  const tmp = `${path}.tmp`;
  writeFileSync(tmp, JSON.stringify(value, null, 2));
  renameSync(tmp, path);
}
