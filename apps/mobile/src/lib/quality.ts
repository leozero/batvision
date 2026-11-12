import {
  parseAutoLevel,
  parseQualityMode,
  type Quality,
  type QualityMode,
} from "@batvision/core/quality";
import AsyncStorage from "@react-native-async-storage/async-storage";

export * from "@batvision/core/quality";

const MODE_KEY = "bv.quality";
const AUTO_KEY = "bv.quality.auto";

/** The picture quality choice is kept per device, as on the web (D60). */
export const readQualityMode = async (): Promise<QualityMode> =>
  parseQualityMode(await AsyncStorage.getItem(MODE_KEY).catch(() => null));

export const saveQualityMode = (mode: QualityMode) =>
  AsyncStorage.setItem(MODE_KEY, mode).catch(() => {});

export const readAutoLevel = async (): Promise<Quality> =>
  parseAutoLevel(await AsyncStorage.getItem(AUTO_KEY).catch(() => null));

export const saveAutoLevel = (level: Quality) =>
  AsyncStorage.setItem(AUTO_KEY, level).catch(() => {});
