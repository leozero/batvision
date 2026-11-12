import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, statSync } from "node:fs";
import { join, resolve, sep } from "node:path";
import type { LocalMeta } from "@batvision/bridge";
import { readJson, writeJson } from "./json-file";
import type { LocalRecord } from "./planner";

export const VIDEO = "video.mp4";
export const PARTIAL = "video.mp4.part";
const MANIFEST = "batvision.json";

/** Images are stored as file names next to the video; URLs are built when served. */
export type StoredMeta = Omit<LocalMeta, "poster" | "backdrop" | "still"> & {
  poster: string | null;
  backdrop: string | null;
  still: string | null;
};

export interface StoredItem {
  key: string;
  kind: "movie" | "episode";
  id: number;
  complete: boolean;
  variantId: number | null;
  profile: string;
  audioLanguage: string | null;
  /** Final size (complete) or expected size (partial). */
  sizeBytes: number;
  subtitles: { file: string; language: string | null; kind: string }[];
  meta: StoredMeta;
  positionMs: number;
  played: boolean;
  /** Unix seconds of the last local or server position update. */
  positionAt: number;
  downloadedAt: number | null;
}

/** What's downloaded, in `<folder>/batvision.json`, one sub-folder per item. */
export class Library {
  private items = new Map<string, StoredItem>();

  constructor(public folder: string) {
    this.load();
  }

  private load() {
    const data = readJson<{ items: StoredItem[] }>(join(this.folder, MANIFEST), { items: [] });
    this.items = new Map(data.items.map((i) => [i.key, i]));
    // Forget items whose files were deleted by hand.
    for (const item of this.items.values()) {
      const file = join(this.dir(item.key), item.complete ? VIDEO : PARTIAL);
      if (!existsSync(file)) this.items.delete(item.key);
    }
  }

  save() {
    writeJson(join(this.folder, MANIFEST), { items: [...this.items.values()] });
  }

  all() {
    return [...this.items.values()];
  }

  get(key: string) {
    return this.items.get(key);
  }

  put(item: StoredItem) {
    this.items.set(item.key, item);
    this.save();
  }

  /** Folder of an item; keys come from the server, so they're checked. */
  dir(key: string) {
    if (!/^(movie|episode)-\d+$/.test(key)) throw new Error(`bad item key: ${key}`);
    return join(this.folder, key);
  }

  /** A file inside an item's folder, or `null` if the path escapes it. */
  file(key: string, name: string): string | null {
    const dir = this.dir(key);
    const path = resolve(dir, name);
    return path.startsWith(dir + sep) ? path : null;
  }

  ensureDir(key: string) {
    const dir = this.dir(key);
    mkdirSync(dir, { recursive: true });
    return dir;
  }

  remove(key: string) {
    this.items.delete(key);
    rmSync(this.dir(key), { recursive: true, force: true });
    this.save();
  }

  /** Bytes on disk for each item, as the planner wants them. */
  records(): Map<string, LocalRecord> {
    const out = new Map<string, LocalRecord>();
    for (const item of this.items.values()) {
      let size = item.sizeBytes;
      if (!item.complete) {
        try {
          size = statSync(join(this.dir(item.key), PARTIAL)).size;
        } catch {
          size = 0;
        }
      }
      out.set(item.key, {
        key: item.key,
        complete: item.complete,
        variantId: item.variantId,
        profile: item.profile,
        audioLanguage: item.audioLanguage,
        sizeBytes: size,
      });
    }
    return out;
  }

  usedBytes() {
    let total = 0;
    for (const record of this.records().values()) total += record.sizeBytes;
    return total;
  }

  /** Moves every item and the manifest to another folder. */
  moveTo(target: string) {
    mkdirSync(target, { recursive: true });
    for (const name of [
      ...readdirSync(this.folder).filter((n) => /^(movie|episode)-\d+$/.test(n)),
      MANIFEST,
    ]) {
      const from = join(this.folder, name);
      if (!existsSync(from)) continue;
      cpSync(from, join(target, name), { recursive: true });
      rmSync(from, { recursive: true, force: true });
    }
    this.folder = target;
    this.load();
  }
}
