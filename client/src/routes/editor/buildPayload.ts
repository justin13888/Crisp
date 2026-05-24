/**
 * Pure transform: validated `RecipeDraft` → `BundlePayload`.
 *
 * Every optional field is emitted only when it carries content, so the wire
 * shape stays minimal and `serde`'s `skip_serializing_if` mirrors what arrives
 * back from the server. Localized values are wrapped with the draft's chosen
 * language; the form authors a single language at a time.
 */

import type { CrispIdentity } from "../../lib/crisp";
import type {
  BundlePayload,
  Equipment,
  Ingredient,
  LocalizedProse,
  LocalizedText,
  Nutrition,
  Quantity,
  Rational,
  Recipe,
  Step,
} from "../../lib/types";
import { amountToQuantity, birthNonce } from "./fields";
import type { RecipeDraft } from "./state";
import { splitCommas } from "./validate";

/** Build-context — everything `buildPayload` needs that is not in `draft`. */
export interface BuildContext {
  identity: CrispIdentity;
  /** Display name to record as `meta.authors[0].display_name`. */
  username: string | null;
  /** Format version reported by `crispFormatVersion()`. */
  formatVersion: number;
  /** Epoch seconds — stamped into every `*_at` field. */
  now: number;
}

/** Builds a complete unsigned bundle payload from the draft. */
export function buildPayload(
  draft: RecipeDraft,
  ctx: BuildContext,
): BundlePayload {
  const lang = draft.language.trim() || "en";
  const title = draft.title.trim();

  // ---- Ingredients ---------------------------------------------------------
  // Filter out blank rows so the user can leave empty rows in the form; build
  // a map of stable client ids → final ref values so step refs can resolve.
  const filledIngredients = draft.ingredients.filter((row) => row.name.trim());
  const ingredientRefById = new Map<number, number>();
  const ingredients: Ingredient[] = filledIngredients.map((row, index) => {
    const ref = index + 1;
    ingredientRefById.set(row.id, ref);
    const ingredient: Ingredient = { ref, name: localized(lang, row.name) };
    const quantity = amountToQuantity(row.amount, row.unit, lang);
    if (quantity) ingredient.quantity = quantity;
    if (row.display.trim()) ingredient.display = localized(lang, row.display);
    if (row.preparation.trim())
      ingredient.preparation = localized(lang, row.preparation);
    if (row.optional) ingredient.optional = true;
    if (row.group !== null && row.group >= 0) ingredient.group = row.group;
    const substitutes = row.substitutes
      .filter((s) => s.name.trim())
      .map((s) => {
        const sub: { name: LocalizedText; quantity?: Quantity } = {
          name: localized(lang, s.name),
        };
        const q = amountToQuantity(s.amount, s.unit, lang);
        if (q) sub.quantity = q;
        return sub;
      });
    if (substitutes.length > 0) ingredient.substitutes = substitutes;
    return ingredient;
  });

  // ---- Equipment -----------------------------------------------------------
  const filledEquipment = draft.equipment.filter((row) => row.name.trim());
  const equipmentRefById = new Map<number, number>();
  const equipment: Equipment[] = filledEquipment.map((row, index) => {
    const ref = index + 1;
    equipmentRefById.set(row.id, ref);
    const item: Equipment = { ref, name: localized(lang, row.name) };
    if (row.note.trim()) item.note = localized(lang, row.note);
    return item;
  });

  // ---- Steps ---------------------------------------------------------------
  const filledSteps = draft.steps.filter((s) => s.text.trim());
  const steps: Step[] = filledSteps.map((row, index) => {
    const step: Step = {
      ref: index + 1,
      text: localizedProse(lang, row.text),
    };
    if (row.group.trim()) step.group = localized(lang, row.group);
    const duration = amountToQuantity(
      row.durationAmount,
      row.durationUnit,
      lang,
    );
    if (duration) step.duration = duration;
    if (row.temperatureValue.trim() && row.temperatureUnit.trim()) {
      const value = parseRational(row.temperatureValue);
      if (value) step.temperature = { value, unit: row.temperatureUnit.trim() };
    }
    if (row.produces.trim()) step.produces = localized(lang, row.produces);
    const ingredientRefs = row.ingredientIds
      .map((id) => ingredientRefById.get(id))
      .filter((ref): ref is number => ref !== undefined);
    if (ingredientRefs.length > 0) step.ingredient_refs = ingredientRefs;
    const equipmentRefs = row.equipmentIds
      .map((id) => equipmentRefById.get(id))
      .filter((ref): ref is number => ref !== undefined);
    if (equipmentRefs.length > 0) step.equipment_refs = equipmentRefs;
    return step;
  });

  // ---- Recipe assembly -----------------------------------------------------
  const recipe: Recipe = {
    identity: {
      author_fpr: new Uint8Array(32),
      title,
      language: lang,
      created_at: ctx.now,
      birth_nonce: birthNonce(),
    },
    id: "",
    revision: "",
    modified_at: ctx.now,
    title: localized(lang, title),
    yield: {
      amount: amountToQuantity(draft.yieldAmount, "", lang) ?? {
        min: [1, 1] as Rational,
      },
      unit_text: localized(lang, draft.yieldUnit.trim() || "servings"),
    },
    ingredients,
    steps,
  };

  if (draft.version.trim()) recipe.version = Number(draft.version);
  if (draft.summary.trim())
    recipe.summary = localizedProse(lang, draft.summary);
  if (draft.notes.trim()) recipe.notes = localizedProse(lang, draft.notes);
  if (draft.attribution.trim())
    recipe.attribution = localizedProse(lang, draft.attribution);

  // Named-phase durations.
  const times: Record<string, Quantity> = {};
  for (const t of draft.times) {
    const phase = t.phase.trim();
    if (!phase || !t.amount.trim()) continue;
    const q = amountToQuantity(t.amount, t.unit, lang);
    if (q) times[phase] = q;
  }
  if (Object.keys(times).length > 0) recipe.times = times;

  // Ingredient groups.
  const groups = draft.ingredientGroups
    .map((g) => g.trim())
    .filter(Boolean)
    .map((g) => localized(lang, g));
  if (groups.length > 0) recipe.ingredient_groups = groups;

  if (equipment.length > 0) recipe.equipment = equipment;

  const tagList = splitCommas(draft.tags);
  if (tagList.length > 0) recipe.tags = tagList;
  const cuisineList = splitCommas(draft.cuisine);
  if (cuisineList.length > 0) recipe.cuisine = cuisineList;
  if (draft.course.length > 0) recipe.course = [...draft.course];
  if (draft.diet.length > 0) recipe.diet = [...draft.diet];
  if (draft.allergens.length > 0) recipe.allergens = [...draft.allergens];

  // Nutrition.
  const nutrition = buildNutrition(draft, lang);
  if (nutrition) recipe.nutrition = nutrition;

  return {
    v: ctx.formatVersion,
    min_v: ctx.formatVersion,
    kind: "recipe",
    doc_id: "",
    default_language: lang,
    languages: [lang],
    meta: {
      title: localized(lang, title),
      authors: [
        {
          uid: ctx.identity.userId,
          display_name: ctx.username ?? "Author",
          keyset: ctx.identity.publicKeyset,
        },
      ],
      license: draft.license.trim() || "NONE",
      created_at: ctx.now,
      modified_at: ctx.now,
    },
    recipes: [recipe],
  };
}

/** Wraps a single string in a single-language localized map. */
function localized(lang: string, text: string): LocalizedText {
  return { [lang]: text };
}

/**
 * Wraps free-form text into the schema's single-paragraph, single-run
 * `LocalizedProse` shape. Rich text editing is out of scope for the form.
 */
function localizedProse(lang: string, text: string): LocalizedProse {
  return { [lang]: [[{ t: text.trim() }]] };
}

/** Parses a decimal string into a `Rational`, or returns `undefined`. */
function parseRational(raw: string): Rational | undefined {
  const trimmed = raw.trim();
  const value = Number(trimmed);
  if (!Number.isFinite(value)) return undefined;
  const decimals = trimmed.includes(".")
    ? (trimmed.split(".")[1]?.length ?? 0)
    : 0;
  const denominator = 10 ** decimals;
  return [Math.round(value * denominator), denominator];
}

/** Builds a `Nutrition` block, or `undefined` if nothing is filled in. */
function buildNutrition(
  draft: RecipeDraft,
  lang: string,
): Nutrition | undefined {
  const n = draft.nutrition;
  const basis = amountToQuantity(n.basisAmount, n.basisUnit, lang);
  const values: Record<string, Quantity> = {};
  for (const [code, v] of Object.entries(n.values)) {
    if (!v.amount.trim()) continue;
    const q = amountToQuantity(v.amount, v.unit, lang);
    if (q) values[code] = q;
  }
  if (!basis && Object.keys(values).length === 0) return undefined;
  if (!basis) return undefined; // basis is required by the schema
  return { basis, values };
}
