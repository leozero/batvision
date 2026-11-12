import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { en } from "./en";
import { fr } from "./fr";

export const UI_LANGUAGES = ["fr", "en"] as const;
export type UiLanguage = (typeof UI_LANGUAGES)[number];

/** UI language for a tag like `fr-FR` (English for anything unsupported). */
export function uiLanguageFor(tag: string | null | undefined): UiLanguage {
  const base = (tag ?? "").slice(0, 2).toLowerCase();
  return (UI_LANGUAGES as readonly string[]).includes(base) ? (base as UiLanguage) : "en";
}

/** Metadata language (TMDB tag) matching a UI language. */
export const METADATA_LANGUAGE: Record<UiLanguage, string> = { fr: "fr-FR", en: "en-US" };

void i18n.use(initReactI18next).init({
  resources: { fr: { translation: fr }, en: { translation: en } },
  lng: uiLanguageFor(typeof navigator === "undefined" ? "en" : navigator.language),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
