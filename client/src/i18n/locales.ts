/**
 * The interface locales the client ships, and how one is chosen.
 *
 * This is the language of the app's own chrome (buttons, labels, messages).
 * Which language a *recipe* is shown in is a separate choice that crisp-core
 * makes from the recipe's own translations (see `lib/crisp.ts`).
 */

/** Every interface locale with a catalogue, fallback first. */
export const SUPPORTED_LOCALES = ["en", "fr"] as const;

/** An interface locale the client has a catalogue for. */
export type Locale = (typeof SUPPORTED_LOCALES)[number];

/** The locale every missing string falls back to. */
export const FALLBACK_LOCALE: Locale = "en";

/** Each locale's name in its own language, for the language picker. */
export const LOCALE_NAMES: Readonly<Record<Locale, string>> = {
  en: "English",
  fr: "Français",
};

/** Where the user's explicit interface-language choice is kept. */
export const LOCALE_STORAGE_KEY = "crisp:locale";

/** Whether a value names a supported interface locale. */
export function isLocale(value: unknown): value is Locale {
  return (
    typeof value === "string" &&
    (SUPPORTED_LOCALES as readonly string[]).includes(value)
  );
}

/**
 * The first supported locale among the preferred language tags, matched on
 * the primary subtag (`fr-CA` → `fr`), or the fallback when none matches.
 */
export function matchLocale(preferred: readonly string[]): Locale {
  for (const tag of preferred) {
    const primary = tag.split("-")[0]?.trim().toLowerCase();
    if (isLocale(primary)) return primary;
  }
  return FALLBACK_LOCALE;
}

/** The locale the user picked in Settings, or `null` to follow the system. */
export function storedLocale(): Locale | null {
  try {
    const value = globalThis.localStorage?.getItem(LOCALE_STORAGE_KEY);
    return isLocale(value) ? value : null;
  } catch {
    return null;
  }
}

/** Remember an explicit choice, or forget it (`null`) to follow the system. */
export function storeLocale(locale: Locale | null): void {
  try {
    if (locale) globalThis.localStorage?.setItem(LOCALE_STORAGE_KEY, locale);
    else globalThis.localStorage?.removeItem(LOCALE_STORAGE_KEY);
  } catch {
    // Storage can be unavailable (private mode); the choice then lasts
    // only for this session.
  }
}

/** The browser's preferred language tags, most preferred first. */
export function systemLanguages(): readonly string[] {
  const nav = globalThis.navigator;
  if (!nav) return [];
  return nav.languages?.length ? nav.languages : [nav.language];
}

/** The locale to start in: the stored choice, else the system's. */
export function initialLocale(): Locale {
  return storedLocale() ?? matchLocale(systemLanguages());
}
