import { hostname } from "node:os";
import { join, resolve } from "node:path";
import type { DesktopInfo, DesktopSettings, SetServerResult } from "@batvision/bridge";
import { resources as translations, uiLanguageFor } from "@batvision/i18n";
import {
  app,
  BrowserWindow,
  dialog,
  ipcMain,
  Menu,
  nativeImage,
  net,
  powerMonitor,
  shell,
  Tray,
} from "electron";
import { Config } from "./config";
import { Engine } from "./engine";
import { APP_URL, handleProtocol, registerScheme } from "./protocol";
import { fetchWithTimeout, normalizeServer, ServerClient } from "./server";
import { Updater } from "./updater";

// Tests run several instances side by side, each with its own data. Otherwise the folder
// keeps the name it had before the app was called BatVision (case matters on Linux).
app.setPath(
  "userData",
  process.env.BATVISION_USER_DATA
    ? resolve(process.env.BATVISION_USER_DATA)
    : join(app.getPath("appData"), "batvision"),
);
if (process.env.BATVISION_VIDEOS_DIR)
  app.setPath("videos", resolve(process.env.BATVISION_VIDEOS_DIR));

// Downloads hold every audio track (D40): the player switches them through
// `HTMLMediaElement.audioTracks`, which Chromium keeps behind this flag.
app.commandLine.appendSwitch("enable-blink-features", "AudioVideoTracks");

registerScheme();

if (!app.requestSingleInstanceLock()) app.quit();

const resources = app.isPackaged ? process.resourcesPath : join(__dirname, "../../resources");
const webDir = resolve(
  process.env.BATVISION_WEB_DIR ??
    (app.isPackaged ? join(process.resourcesPath, "web") : join(__dirname, "../../../web/dist")),
);

const OS: DesktopInfo["os"] =
  process.platform === "darwin" ? "mac" : process.platform === "win32" ? "windows" : "linux";
const OS_NAME = { mac: "macOS", windows: "Windows", linux: "Linux" }[OS];

let window: BrowserWindow | null = null;
let tray: Tray | null = null;
let quitting = false;

app.whenReady().then(() => {
  const config = new Config();
  const server = new ServerClient(config, () => signedOut());
  const engine = new Engine(config, server);
  const updater = new Updater();
  let reachable = true;

  const signedOut = () => {
    engine.reset();
    updateTray();
  };

  handleProtocol({
    webDir,
    config,
    library: () => engine.library,
    onSignedIn: () => {
      void engine.sync();
      updateTray();
    },
    onSignedOut: signedOut,
    onReachable: (now) => {
      // The server is back: upload offline progress and catch up.
      if (now && !reachable) void engine.sync();
      reachable = now;
    },
  });

  // ---- Bridge ----------------------------------------------------------------------------
  const settings = (): DesktopSettings => ({
    folder: engine.library.folder,
    launchAtLogin: config.launchAtLogin,
  });

  ipcMain.handle(
    "bv:info",
    (): DesktopInfo => ({
      version: app.getVersion(),
      os: OS,
      server: config.server,
      signedIn: Boolean(config.token),
      device: { name: `${hostname().replace(/\.local$/, "")} · ${OS_NAME}`, platform: "desktop" },
    }),
  );
  ipcMain.handle("bv:setServer", async (_e, input: string): Promise<SetServerResult> => {
    const origin = normalizeServer(String(input));
    // Signed in, the server can't change: the token would follow to the new address.
    if (!origin || config.token) return { ok: false, error: "invalid_url" };
    try {
      const response = await fetchWithTimeout(`${origin}/api/health`, {}, 10_000);
      const body = (await response.json().catch(() => null)) as { status?: string } | null;
      if (!response.ok || body?.status !== "ok") return { ok: false, error: "not_batvision" };
    } catch {
      return { ok: false, error: "unreachable" };
    }
    config.server = origin;
    return { ok: true };
  });
  ipcMain.handle("bv:forgetServer", () => {
    if (config.token) return;
    config.server = null;
  });
  ipcMain.handle("bv:downloads", () => engine.state());
  ipcMain.handle("bv:syncNow", () => engine.sync());
  ipcMain.handle("bv:setPaused", (_e, paused: boolean) => {
    engine.setPaused(Boolean(paused));
    updateTray();
  });
  ipcMain.handle("bv:cancel", (_e, kind: "movie" | "episode", id: number) =>
    engine.cancel(kind === "episode" ? "episode" : "movie", Number(id)),
  );
  ipcMain.handle("bv:setAv1", (_e, av1: boolean) => {
    if (config.av1 !== Boolean(av1)) config.av1 = Boolean(av1);
  });
  ipcMain.handle("bv:localPlayback", (_e, kind: "movie" | "episode", id: number) =>
    engine.localPlayback(kind === "episode" ? "episode" : "movie", Number(id)),
  );
  ipcMain.handle("bv:reportProgress", (_e, report) => engine.reportProgress(report));
  ipcMain.handle("bv:settings", settings);
  ipcMain.handle("bv:setLaunchAtLogin", (_e, enabled: boolean) => {
    config.launchAtLogin = Boolean(enabled);
    if (app.isPackaged)
      app.setLoginItemSettings({
        openAtLogin: config.launchAtLogin,
        args: ["--hidden"],
      });
    return settings();
  });
  ipcMain.handle("bv:chooseFolder", async () => {
    const result = await dialog.showOpenDialog({
      defaultPath: engine.library.folder,
      properties: ["openDirectory", "createDirectory"],
    });
    const folder = result.filePaths[0];
    if (!result.canceled && folder && folder !== engine.library.folder)
      await engine.setFolder(folder);
    return settings();
  });
  ipcMain.handle("bv:openFolder", () => shell.openPath(engine.library.folder));
  ipcMain.handle("bv:update", () => updater.state());
  ipcMain.handle("bv:installUpdate", () => installUpdate());

  engine.on("state", (state) => window?.webContents.send("bv:downloads", state));
  updater.on("state", (state) => {
    window?.webContents.send("bv:update", state);
    updateTray();
  });

  const installUpdate = () => {
    if (updater.state().status === "ready") quitting = true;
    updater.install();
  };

  // ---- Window and tray -----------------------------------------------------------------
  const show = () => {
    if (!window) createWindow();
    else {
      if (window.isMinimized()) window.restore();
      window.show();
      window.focus();
    }
  };

  function createWindow() {
    window = new BrowserWindow({
      width: 1280,
      height: 820,
      minWidth: 900,
      minHeight: 600,
      backgroundColor: "#0b0b0f",
      title: "BatVision",
      icon: join(resources, "icon.png"),
      show: false,
      webPreferences: {
        preload: join(__dirname, "../preload/index.js"),
        contextIsolation: true,
        sandbox: true,
        nodeIntegration: false,
      },
    });
    window.once("ready-to-show", () => window?.show());
    window.on("close", (event) => {
      // Closing keeps the app in the tray: downloads go on.
      if (!quitting) {
        event.preventDefault();
        window?.hide();
      }
    });
    window.on("closed", () => {
      window = null;
    });
    window.webContents.setWindowOpenHandler(({ url }) => {
      if (/^https?:\/\//.test(url)) void shell.openExternal(url);
      return { action: "deny" };
    });
    window.webContents.on("will-navigate", (event, url) => {
      if (!url.startsWith(APP_URL)) event.preventDefault();
    });
    void window.loadURL(APP_URL);
  }

  function updateTray() {
    if (!tray) return;
    // The account's language lives in the page; the main process only knows the system's.
    const labels = translations[uiLanguageFor(app.getLocale())].translation.desktop.tray;
    const update = updater.state();
    const versioned = (label: string, version: string) => label.replace("{{version}}", version);
    const updateItems: Electron.MenuItemConstructorOptions[] =
      update.status === "ready"
        ? [{ label: versioned(labels.restartToUpdate, update.version), click: installUpdate }]
        : update.status === "available"
          ? [{ label: versioned(labels.downloadUpdate, update.version), click: installUpdate }]
          : [];
    tray.setContextMenu(
      Menu.buildFromTemplate([
        { label: labels.open, click: show },
        ...updateItems,
        { type: "separator" },
        {
          label: labels.syncNow,
          enabled: Boolean(config.token),
          click: () => void engine.sync(),
        },
        {
          label: config.paused ? labels.resume : labels.pause,
          enabled: Boolean(config.token),
          click: () => {
            engine.setPaused(!config.paused);
            updateTray();
          },
        },
        { type: "separator" },
        {
          label: labels.quit,
          click: () => {
            quitting = true;
            app.quit();
          },
        },
      ]),
    );
  }

  const trayIcon = nativeImage.createFromPath(
    join(resources, OS === "mac" ? "trayTemplate.png" : "tray.png"),
  );
  if (OS === "mac") trayIcon.setTemplateImage(true);
  tray = new Tray(trayIcon);
  tray.setToolTip("BatVision");
  tray.on("click", show);
  updateTray();

  app.on("second-instance", show);
  app.on("activate", show);
  app.on("before-quit", () => {
    quitting = true;
    engine.stop();
  });

  // ---- Sync triggers: on open, when the network comes back, after sleep ----------------
  let online = net.isOnline();
  setInterval(() => {
    const now = net.isOnline();
    if (now && !online) void engine.sync();
    online = now;
  }, 30_000);
  powerMonitor.on("resume", () => void engine.sync());

  if (!process.argv.includes("--hidden")) createWindow();
  engine.start();
  updater.start();
});

// Closing every window doesn't quit: the tray keeps the app alive.
app.on("window-all-closed", () => {});
