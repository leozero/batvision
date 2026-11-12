import { BatvisionSync } from "../../modules/batvision-sync";

export { normalizeServer } from "./address";

/** Server address and device token, kept by the native engine (token in the Android keystore). */
export function session() {
  return BatvisionSync.getSession();
}

/** "Pixel 8 · Android": how this phone appears in the device list (D72). */
export function deviceName() {
  const { model } = BatvisionSync.deviceInfo();
  return `${model} · Android`;
}
