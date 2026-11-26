/**
 * Suggesting a missing ingredient to the ingredient index from the editor
 * (`docs/ingredient-index.md` §6): what a typed name proposes, and why a
 * server refused it.
 */
import { i18n } from "../../i18n";
import {
  errorMessage,
  type IngredientName,
  type IngredientSuggestion,
  isCrispFailure,
} from "../../lib/crisp";

/** The part of the engine a suggestion needs (stubbed in tests). */
export interface SuggestEngine {
  /** The shard a recipe language's names are filed under, if any. */
  ingredientShard(tag: string): string | undefined;
  /** The index's name for a slug in a language, following fallbacks. */
  ingredientName(slug: string, tag: string): IngredientName | undefined;
}

/** The longest text the server accepts in a suggestion. */
export const MAX_SUGGESTION_TEXT = 200;

/** The failure `code` of a suggestion over the account's budget. */
export const TOO_MANY_REQUESTS = "too-many-requests";

/**
 * What the author's typed `name` (linked to `code`, if any) in a recipe in
 * language `lang` can propose to the index:
 * - an unlinked name proposes a `new_entry` under that name;
 * - a linked entry with no name in the recipe's own shard (its chip shows
 *   a fallback language) proposes the typed name as its `name` there;
 * - nothing otherwise: an empty or over-long name, an entry already named
 *   in that shard, or a language the index has no shard for.
 */
export function suggestionFor(
  engine: SuggestEngine,
  lang: string,
  name: string,
  code: string | undefined,
): IngredientSuggestion | undefined {
  const text = name.trim();
  if (!text || [...text].length > MAX_SUGGESTION_TEXT) return undefined;
  const locale = engine.ingredientShard(lang);
  if (!locale) return undefined;
  if (!code) return { kind: "new_entry", locale, text };
  if (engine.ingredientName(code, lang)?.shard === locale) return undefined;
  return { kind: "name", locale, text, slug: code };
}

/**
 * Why a suggestion failed, for the author: an exhausted budget says to
 * wait; anything else (an entry that already has the name, the cap on open
 * suggestions, a name the index rejects) is the server's own message.
 */
export function suggestionFailureMessage(cause: unknown): string {
  if (isCrispFailure(cause) && cause.code === TOO_MANY_REQUESTS) {
    return i18n.t("editorLists:link.suggest.tooMany");
  }
  return errorMessage(cause);
}
