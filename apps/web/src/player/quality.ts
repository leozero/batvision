import {
  parseAutoLevel,
  parseQualityMode,
  type Quality,
  type QualityMode,
} from "@batvision/core/quality";

export * from "@batvision/core/quality";

const MODE_KEY = "bv.quality";
const AUTO_KEY = "bv.quality.auto";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // ignore
  }
}

/** The choice is kept per device: the network depends on where it is. */
export const readQualityMode = (): QualityMode => parseQualityMode(read(MODE_KEY));

export const saveQualityMode = (mode: QualityMode) => write(MODE_KEY, mode);

/** Auto starts from where it settled last time on this device. */
export const readAutoLevel = (): Quality => parseAutoLevel(read(AUTO_KEY));

export const saveAutoLevel = (level: Quality) => write(AUTO_KEY, level);
