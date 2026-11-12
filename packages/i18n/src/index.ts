/** Interface translations shared by the web, desktop and Android apps (D61). */
import { en } from "./en";
import { fr } from "./fr";

export { en, fr };

export const UI_LANGUAGES = ["fr", "en"] as const;
export type UiLanguage = (typeof UI_LANGUAGES)[number];

/** i18next resources for every UI language. */
export const resources = { fr: { translation: fr }, en: { translation: en } } as const;

/** UI language for a tag like `fr-FR` (English for anything unsupported). */
export function uiLanguageFor(tag: string | null | undefined): UiLanguage {
  const base = (tag ?? "").slice(0, 2).toLowerCase();
  return (UI_LANGUAGES as readonly string[]).includes(base) ? (base as UiLanguage) : "en";
}

/** Metadata language (TMDB tag) matching a UI language. */
export const METADATA_LANGUAGE: Record<UiLanguage, string> = { fr: "fr-FR", en: "en-US" };
