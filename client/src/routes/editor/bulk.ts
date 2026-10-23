/**
 * "Paste a list" bulk entry: turning pasted text into ingredient rows and
 * steps. Whether a token run is an amount is the engine's call, injected as
 * an {@link IsAmount} predicate.
 */

import type { ParsedAmount } from "../../generated/crisp";

/** Whether `text` reads as a complete amount (number plus known unit). */
export type IsAmount = (text: string) => boolean;

/** One pasted ingredient. */
export interface BulkIngredient {
  /** The leading amount, as typed (may be empty). */
  amount: string;
  /** The ingredient name. */
  name: string;
  /** Text after the first comma, e.g. `finely chopped`. */
  preparation: string;
}

/** A run of pasted ingredients under an optional section title. */
export interface BulkSection {
  /** The section title (`For the sauce`), or `null` before any title. */
  title: string | null;
  /** The ingredients in this section. */
  items: BulkIngredient[];
}

/** The most tokens an amount may span (`about 1 1/2 cups`). */
const MAX_AMOUNT_TOKENS = 4;

/** Builds an {@link IsAmount} from the engine's `parseAmount`. */
export function amountPredicate(
  parseAmount: (text: string, lang: string) => ParsedAmount,
  lang: string,
): IsAmount {
  return (text) => {
    try {
      const { quantity, unit_known } = parseAmount(text, lang);
      return (
        quantity.min !== undefined &&
        (unit_known || quantity.unit === undefined)
      );
    } catch {
      return false;
    }
  };
}

/** Strips list bullets (`-`, `*`, `•`) and leading numbering (`1.`, `2)`). */
export function stripBullet(line: string): string {
  return line.replace(/^\s*(?:[-*•·▪◦]+|\d+[.)](?=\s))\s*/u, "").trim();
}

/** Splits one ingredient line into amount, name, and preparation. */
export function parseIngredientLine(
  line: string,
  isAmount: IsAmount,
): BulkIngredient {
  const text = stripBullet(line);
  const tokens = text.split(/\s+/).filter(Boolean);
  let amount = "";
  let rest = text;
  const longest = Math.min(MAX_AMOUNT_TOKENS, Math.max(tokens.length - 1, 1));
  for (let k = longest; k >= 1; k--) {
    const candidate = tokens.slice(0, k).join(" ");
    if (isAmount(candidate)) {
      amount = candidate;
      rest = tokens.slice(k).join(" ");
      break;
    }
  }
  rest = rest.replace(/^of\s+/i, "");
  const comma = rest.indexOf(",");
  const name = (comma < 0 ? rest : rest.slice(0, comma)).trim();
  const preparation = comma < 0 ? "" : rest.slice(comma + 1).trim();
  return { amount, name, preparation };
}

/**
 * Parses a pasted ingredient list: one ingredient per line; a line ending in
 * `:` (`For the sauce:`) starts a new section.
 */
export function parseIngredientList(
  text: string,
  isAmount: IsAmount,
): BulkSection[] {
  const sections: BulkSection[] = [{ title: null, items: [] }];
  for (const raw of text.replace(/\r\n?/g, "\n").split("\n")) {
    const line = stripBullet(raw);
    if (!line) continue;
    if (line.endsWith(":")) {
      const title = line.slice(0, -1).trim();
      if (title) {
        sections.push({ title, items: [] });
        continue;
      }
    }
    const item = parseIngredientLine(line, isAmount);
    if (item.name || item.amount) sections.at(-1)?.items.push(item);
  }
  return sections.filter((s) => s.title !== null || s.items.length > 0);
}

/** Strips step numbering (`1.`, `2)`, `Step 3:`). */
function stripStepNumber(text: string): string {
  return text
    .replace(/^\s*(?:step\s*)?\d+\s*[.):-]\s*/i, "")
    .replace(/^\s*[-*•·]\s+/u, "")
    .trim();
}

/**
 * Parses pasted method text into step texts. Blank lines separate steps
 * when there are any (wrapped lines inside a step are re-joined); otherwise
 * every line is a step.
 */
export function parseStepList(text: string): string[] {
  const normalized = text.replace(/\r\n?/g, "\n").trim();
  if (!normalized) return [];
  const chunks = /\n[ \t]*\n/.test(normalized)
    ? normalized.split(/\n[ \t]*\n/).map((c) => c.replace(/\s*\n\s*/g, " "))
    : normalized.split("\n");
  return chunks.map(stripStepNumber).filter((s) => s.length > 0);
}
