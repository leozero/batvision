import { resources, uiLanguageFor } from "@batvision/i18n";
import i18n from "i18next";
import { initReactI18next } from "react-i18next";

export {
  METADATA_LANGUAGE,
  UI_LANGUAGES,
  type UiLanguage,
  uiLanguageFor,
} from "@batvision/i18n";

void i18n.use(initReactI18next).init({
  resources,
  lng: uiLanguageFor(typeof navigator === "undefined" ? "en" : navigator.language),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
