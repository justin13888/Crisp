/** Editor defaults derived from the cook's locale. */

import type { TemperatureUnit } from "../../generated/crisp";

/** The default temperature unit for a locale (°F only for the US). */
export function defaultTempUnit(locale: string): TemperatureUnit {
  return /-(US|LR|MM)\b/i.test(locale) ? "[degF]" : "Cel";
}

/** The primary language subtag of a locale (`en-US` → `en`). */
export function primaryLanguage(locale: string | undefined): string {
  const primary = (locale ?? "").split("-")[0]?.trim().toLowerCase() ?? "";
  return /^[a-z]{2,3}$/.test(primary) ? primary : "en";
}
