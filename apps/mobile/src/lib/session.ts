import type { Schemas } from "@batvision/api-client";
import { Platform } from "react-native";
import { BatvisionSync } from "../../modules/batvision-sync";
import { IS_TV } from "./tv";

export { isPublicCleartext, normalizeServer } from "./address";

/** Server address and device token, kept by the native engine (token in the Android keystore or the iOS keychain). */
export function session() {
  return BatvisionSync.getSession();
}

/** "Pixel 8 · Android", "SHIELD Android TV · Android TV", "iPad · iPadOS": how this device
 * appears in the device list (D72). */
export function deviceName() {
  const { model } = BatvisionSync.deviceInfo();
  if (IS_TV) return `${model} · Android TV`;
  if (Platform.OS !== "ios") return `${model} · Android`;
  return `${model} · ${model === "iPad" ? "iPadOS" : "iOS"}`;
}

/** What this device is, for the server: a TV doesn't download (D110). */
export function devicePlatform(): Schemas["Platform"] {
  if (IS_TV) return "android_tv";
  return Platform.OS === "ios" ? "ios" : "android";
}
