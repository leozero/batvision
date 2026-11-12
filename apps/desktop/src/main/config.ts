import { join } from "node:path";
import { app, safeStorage } from "electron";
import { readJson, writeJson } from "./json-file";

/** An offline progress record, as `POST /api/progress/batch` takes it. */
export interface PendingProgress {
  kind: "movie" | "episode";
  id: number;
  position_ms: number;
  played: boolean;
  /** Unix seconds. */
  at: number;
}

interface ConfigData {
  server: string | null;
  /** Device token, encrypted with the OS keychain (`plain:` prefix when unavailable). */
  token: string | null;
  folder: string;
  launchAtLogin: boolean;
  av1: boolean | null;
  paused: boolean;
  pendingProgress: PendingProgress[];
}

/** App settings and credentials, in `userData/config.json`. */
export class Config {
  private path = join(app.getPath("userData"), "config.json");
  private data: ConfigData;

  constructor() {
    this.data = readJson<ConfigData>(this.path, {
      server: null,
      token: null,
      folder: join(app.getPath("videos"), "Batvision"),
      launchAtLogin: false,
      av1: null,
      paused: false,
      pendingProgress: [],
    });
  }

  private save() {
    writeJson(this.path, this.data);
  }

  get server() {
    return this.data.server;
  }
  set server(value: string | null) {
    this.data.server = value;
    this.save();
  }

  get token(): string | null {
    const stored = this.data.token;
    if (!stored) return null;
    if (stored.startsWith("plain:")) return stored.slice(6);
    try {
      return safeStorage.decryptString(Buffer.from(stored, "base64"));
    } catch {
      return null;
    }
  }
  set token(value: string | null) {
    if (!value) this.data.token = null;
    else if (safeStorage.isEncryptionAvailable())
      this.data.token = safeStorage.encryptString(value).toString("base64");
    else {
      console.warn("OS keychain unavailable: the device token is stored unencrypted");
      this.data.token = `plain:${value}`;
    }
    this.save();
  }

  get folder() {
    return this.data.folder;
  }
  set folder(value: string) {
    this.data.folder = value;
    this.save();
  }

  get launchAtLogin() {
    return this.data.launchAtLogin;
  }
  set launchAtLogin(value: boolean) {
    this.data.launchAtLogin = value;
    this.save();
  }

  get av1() {
    return this.data.av1;
  }
  set av1(value: boolean | null) {
    this.data.av1 = value;
    this.save();
  }

  get paused() {
    return this.data.paused;
  }
  set paused(value: boolean) {
    this.data.paused = value;
    this.save();
  }

  get pendingProgress(): readonly PendingProgress[] {
    return this.data.pendingProgress;
  }

  /** Keeps only the latest record per item. */
  addProgress(record: PendingProgress) {
    this.data.pendingProgress = [
      ...this.data.pendingProgress.filter((p) => p.kind !== record.kind || p.id !== record.id),
      record,
    ];
    this.save();
  }

  /** Drops the records that were sent (unless a newer one arrived meanwhile). */
  removeProgress(sent: readonly PendingProgress[]) {
    this.data.pendingProgress = this.data.pendingProgress.filter((p) => !sent.includes(p));
    this.save();
  }
}
