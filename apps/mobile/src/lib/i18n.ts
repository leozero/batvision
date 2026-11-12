import { resources, uiLanguageFor } from "@batvision/i18n";
import { getLocales } from "expo-localization";
import i18n from "i18next";
import { initReactI18next } from "react-i18next";

export { uiLanguageFor };

void i18n.use(initReactI18next).init({
  resources,
  lng: uiLanguageFor(getLocales()[0]?.languageTag),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
