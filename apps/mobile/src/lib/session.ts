import { Platform } from "react-native";
import { BatvisionSync } from "../../modules/batvision-sync";

export { normalizeServer } from "./address";

/** Server address and device token, kept by the native engine (token in the Android keystore or the iOS keychain). */
export function session() {
  return BatvisionSync.getSession();
}

/** "Pixel 8 · Android", "iPad · iPadOS": how this device appears in the device list (D72). */
export function deviceName() {
  const { model } = BatvisionSync.deviceInfo();
  if (Platform.OS !== "ios") return `${model} · Android`;
  return `${model} · ${model === "iPad" ? "iPadOS" : "iOS"}`;
}
