import { EventEmitter } from "node:events";
import type { UpdateState } from "@batvision/bridge";
import { app, shell } from "electron";
import { autoUpdater } from "electron-updater";
import { fetchWithTimeout } from "./server";
import { isNewer } from "./version";

const LATEST_RELEASE = "https://api.github.com/repos/leozero/batvision/releases/latest";
const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

/**
 * Keeps the app up to date from the GitHub releases. Windows and Linux (AppImage)
 * download and install on their own (electron-updater); macOS builds aren't signed,
 * so they can only point to the new release (D58).
 */
export class Updater extends EventEmitter<{ state: [UpdateState] }> {
  private current: UpdateState = { status: "none" };
  /** A release feed for tests: skips electron-updater and checks this URL. */
  private readonly feed = process.env.BATVISION_UPDATE_URL;
  private readonly native =
    !this.feed &&
    app.isPackaged &&
    (process.platform === "win32" || (process.platform === "linux" && !!process.env.APPIMAGE));

  state() {
    return this.current;
  }

  private set(state: UpdateState) {
    this.current = state;
    this.emit("state", state);
  }

  start() {
    // Development builds don't update; tests point `BATVISION_UPDATE_URL` at a fake release.
    if (!app.isPackaged && !this.feed) return;
    if (this.native) {
      autoUpdater.autoInstallOnAppQuit = true;
      autoUpdater.on("download-progress", (p) => {
        const version = "version" in this.current ? this.current.version : "";
        this.set({ status: "downloading", version, percent: Math.round(p.percent) });
      });
      autoUpdater.on("update-available", (info) =>
        this.set({ status: "downloading", version: info.version, percent: 0 }),
      );
      autoUpdater.on("update-downloaded", (info) =>
        this.set({ status: "ready", version: info.version }),
      );
      autoUpdater.on("error", (err) => {
        console.warn("update check failed:", err.message);
        // A download cut short: the next check starts it again.
        if (this.current.status === "downloading") this.set({ status: "none" });
      });
    }
    setTimeout(() => void this.check(), this.feed ? 1000 : 15_000);
    setInterval(() => void this.check(), CHECK_EVERY_MS);
  }

  async check() {
    if (this.current.status === "ready" || this.current.status === "downloading") return;
    try {
      if (this.native) {
        await autoUpdater.checkForUpdates();
        return;
      }
      const response = await fetchWithTimeout(
        this.feed ?? LATEST_RELEASE,
        { headers: { accept: "application/vnd.github+json" } },
        15_000,
      );
      if (!response.ok) return;
      const release = (await response.json()) as { tag_name?: string; html_url?: string };
      if (release.tag_name && release.html_url && isNewer(release.tag_name, app.getVersion()))
        this.set({
          status: "available",
          version: release.tag_name.replace(/^v/, ""),
          url: release.html_url,
        });
    } catch (err) {
      console.warn("update check failed:", (err as Error).message);
    }
  }

  /** Restarts into the downloaded version, or opens the release page. */
  install() {
    if (this.current.status === "ready") autoUpdater.quitAndInstall(true, true);
    else if (this.current.status === "available") void shell.openExternal(this.current.url);
  }
}
