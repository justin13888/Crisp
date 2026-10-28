/**
 * Interface-string localisation.
 *
 * Importing this module initialises the app-wide i18next instance in the
 * user's locale and binds it to React, so components call
 * `useTranslation("<namespace>")` and non-React code calls `i18n.t(...)`.
 */
import { initReactI18next } from "react-i18next";
import { createI18n } from "./instance";
import {
  initialLocale,
  isLocale,
  type Locale,
  matchLocale,
  storeLocale,
  systemLanguages,
} from "./locales";

export type { Locale } from "./locales";
export { LOCALE_NAMES, SUPPORTED_LOCALES, storedLocale } from "./locales";
export { Trans, useTranslation } from "react-i18next";

/** The app-wide instance. */
export const i18n = createI18n(initialLocale(), undefined, [initReactI18next]);

/** Mirror the interface language onto `<html lang>` for assistive tech. */
function syncDocumentLang(lng: string): void {
  if (globalThis.document) document.documentElement.lang = lng;
}
syncDocumentLang(i18n.language);
i18n.on("languageChanged", syncDocumentLang);

/** The interface locale in effect. */
export function currentLocale(): Locale {
  return isLocale(i18n.language) ? i18n.language : matchLocale([i18n.language]);
}

/**
 * Switch the interface language and remember the choice; `null` forgets it
 * and follows the system's preferred languages again.
 */
export function setLocale(locale: Locale | null): void {
  storeLocale(locale);
  void i18n.changeLanguage(locale ?? matchLocale(systemLanguages()));
}
