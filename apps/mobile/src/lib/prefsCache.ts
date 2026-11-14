import { File, Paths } from "expo-file-system";
import type { Schemas } from "./api";

type Preferences = Schemas["Preferences"];

/** The account's preferences as last loaded, so downloads played offline honour them. */
function file() {
  return new File(Paths.document, "preferences.json");
}

export function cachedPreferences(): Preferences | undefined {
  try {
    const f = file();
    return f.exists ? (JSON.parse(f.textSync()) as Preferences) : undefined;
  } catch {
    return undefined;
  }
}

export function cachePreferences(prefs: Preferences) {
  void file()
    .write(JSON.stringify(prefs))
    .catch(() => {});
}
