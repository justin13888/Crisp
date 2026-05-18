/**
 * Pure rendering helpers for the recipe viewer: locale resolution and the
 * formatting of exact rationals and quantities.
 *
 * These never make a trust decision — a bundle must already be verified.
 * Locale fallback follows spec §5: each preferred language in turn, then the
 * bundle default, then any available value.
 */

import type {
  LocalizedProse,
  LocalizedText,
  Prose,
  Quantity,
  Rational,
} from "./types";

/** Picks the best value from a localized map for a reader. */
function pick<T>(
  map: Record<string, T>,
  preferred: string[],
  defaultLanguage: string,
): T | undefined {
  for (const tag of preferred) {
    const value = map[tag];
    if (value !== undefined) return value;
  }
  const fallback = map[defaultLanguage];
  if (fallback !== undefined) return fallback;
  for (const value of Object.values(map)) return value;
  return undefined;
}

/** Resolves a localized single-line string, or `""` if the map is empty. */
export function resolveText(
  text: LocalizedText | undefined,
  preferred: string[],
  defaultLanguage: string,
): string {
  if (!text) return "";
  return pick(text, preferred, defaultLanguage) ?? "";
}

/** Resolves a localized prose block, or an empty block if the map is empty. */
export function resolveProse(
  prose: LocalizedProse | undefined,
  preferred: string[],
  defaultLanguage: string,
): Prose {
  if (!prose) return [];
  return pick(prose, preferred, defaultLanguage) ?? [];
}

/** Greatest common divisor, for reducing a rational to lowest terms. */
function gcd(a: number, b: number): number {
  let x = Math.abs(a);
  let y = Math.abs(b);
  while (y !== 0) {
    [x, y] = [y, x % y];
  }
  return x || 1;
}

/** Common unicode vulgar fractions, for a friendlier amount display. */
const VULGAR: Record<string, string> = {
  "1/2": "½",
  "1/3": "⅓",
  "2/3": "⅔",
  "1/4": "¼",
  "3/4": "¾",
  "1/8": "⅛",
};

/** Formats an exact rational as a readable mixed number, e.g. `1½` or `3/2`. */
export function formatRational(rational: Rational): string {
  const [rawNum, rawDen] = rational;
  if (rawDen <= 0) return String(rawNum);
  const divisor = gcd(rawNum, rawDen);
  const num = rawNum / divisor;
  const den = rawDen / divisor;
  if (den === 1) return String(num);

  const sign = num < 0 ? "-" : "";
  const absNum = Math.abs(num);
  const whole = Math.floor(absNum / den);
  const remainder = absNum % den;
  const fraction = VULGAR[`${remainder}/${den}`] ?? `${remainder}/${den}`;
  return whole > 0 ? `${sign}${whole}${fraction}` : `${sign}${fraction}`;
}

/** Formats a quantity — a number, a range, a unit, and any text amount. */
export function formatQuantity(
  quantity: Quantity | undefined,
  preferred: string[],
  defaultLanguage: string,
): string {
  if (!quantity) return "";
  const parts: string[] = [];
  if (quantity.min) {
    const low = formatRational(quantity.min);
    parts.push(quantity.max ? `${low}–${formatRational(quantity.max)}` : low);
  }
  if (quantity.unit) parts.push(quantity.unit);
  let result = parts.join(" ");
  if (quantity.approx && result) result = `~${result}`;
  const text = resolveText(quantity.text, preferred, defaultLanguage);
  if (text) result = result ? `${result} (${text})` : text;
  return result;
}
