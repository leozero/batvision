import type { DesktopBridge, DownloadsState, UpdateState } from "@batvision/bridge";
import { contextBridge, ipcRenderer } from "electron";

const invoke = (channel: string, ...args: unknown[]) =>
  ipcRenderer.invoke(`bv:${channel}`, ...args);

/** `window.batvision`: what the web interface can ask the desktop app. */
const bridge: DesktopBridge = {
  info: () => invoke("info"),
  setServer: (url) => invoke("setServer", url),
  forgetServer: () => invoke("forgetServer"),
  downloads: () => invoke("downloads"),
  onDownloads: (listener) => {
    const handler = (_event: unknown, state: DownloadsState) => listener(state);
    ipcRenderer.on("bv:downloads", handler);
    return () => ipcRenderer.removeListener("bv:downloads", handler);
  },
  syncNow: () => invoke("syncNow"),
  setPaused: (paused) => invoke("setPaused", paused),
  cancel: (kind, id) => invoke("cancel", kind, id),
  setAv1: (av1) => invoke("setAv1", av1),
  localPlayback: (kind, id) => invoke("localPlayback", kind, id),
  reportProgress: (report) => invoke("reportProgress", report),
  settings: () => invoke("settings"),
  setLaunchAtLogin: (enabled) => invoke("setLaunchAtLogin", enabled),
  chooseFolder: () => invoke("chooseFolder"),
  openFolder: () => invoke("openFolder"),
  update: () => invoke("update"),
  onUpdate: (listener) => {
    const handler = (_event: unknown, state: UpdateState) => listener(state);
    ipcRenderer.on("bv:update", handler);
    return () => ipcRenderer.removeListener("bv:update", handler);
  },
  installUpdate: () => invoke("installUpdate"),
};

contextBridge.exposeInMainWorld("batvision", bridge);
