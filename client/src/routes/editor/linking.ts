/**
 * Linking a typed ingredient name to the ingredient index
 * (`docs/ingredient-index.md` §3.3): an exact match attaches its slug at
 * once, anything else is offered as suggestions the author picks from. The
 * author's `name` is never rewritten; only `code` changes.
 */
import type { IngredientCandidate, IngredientMatch } from "../../lib/crisp";

/** The part of the engine linking needs (stubbed in tests). */
export interface LinkEngine {
  /** Matches a typed name in a language against the index. */
  matchIngredient(text: string, tag: string, limit: number): IngredientMatch;
}

/** How many suggestions a name offers. */
export const SUGGESTION_LIMIT = 5;

/** The slug `name` links to exactly, if any. */
export function exactLink(
  engine: LinkEngine,
  name: string,
  lang: string,
): string | undefined {
  if (!name.trim()) return undefined;
  const found = engine.matchIngredient(name, lang, SUGGESTION_LIMIT);
  return found.kind === "exact" ? found.slug : undefined;
}

/**
 * The code an ingredient carries after its name changes from `before` to
 * `after`:
 * - a name that links exactly to a slug the old name did not attaches it;
 * - a code that was only the old name's automatic link is dropped once the
 *   name no longer links to it;
 * - otherwise the code stays: one the author picked, one the bundle
 *   carried (known to this index or not), or none after the author removed
 *   the chip while the name still links the same way.
 */
export function relink(
  engine: LinkEngine,
  lang: string,
  before: string,
  after: string,
  code: string | undefined,
): string | undefined {
  const was = exactLink(engine, before, lang);
  const now = exactLink(engine, after, lang);
  if (now !== undefined) return now === was ? code : now;
  return code !== undefined && code === was ? undefined : code;
}

/** One entry an author may link an ingredient to. */
export interface Suggestion {
  /** The index slug. */
  slug: string;
  /** Its name in the recipe's language. */
  name: string;
}

/**
 * Suggestions for an unlinked name, best first: the ranked candidates, or
 * the exact link itself when the author removed it (so it can be put back).
 */
export function suggestions(
  engine: LinkEngine,
  name: string,
  lang: string,
): Suggestion[] {
  if (!name.trim()) return [];
  const found = engine.matchIngredient(name, lang, SUGGESTION_LIMIT);
  switch (found.kind) {
    case "exact":
      return [{ slug: found.slug, name: found.name ?? found.slug }];
    case "candidates":
      return found.candidates.map(({ slug, name }: IngredientCandidate) => ({
        slug,
        name,
      }));
    case "none":
      return [];
  }
}
