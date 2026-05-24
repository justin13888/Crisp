/**
 * Pure draft validation.
 *
 * `validateDraft` produces a `Record<fieldPath, message>` map. The form treats
 * the map's emptiness as the submit gate, and each field component reads its
 * slice to render an inline error. Every check below mirrors a structural
 * rule that `crisp-core` would otherwise enforce at signing time — the form
 * surfaces them up front so the user never produces an invalid bundle.
 */

import type { RecipeConstraints } from "../../lib/types";
import { LANGUAGES, LICENSES } from "./curated";
import { byteLength } from "./fields";
import type { RecipeDraft } from "./state";

/** A map of field-path strings to human-readable error messages. */
export type ErrorMap = Record<string, string>;

/** Pure validator: never touches state, returns one error map per call. */
export function validateDraft(
  draft: RecipeDraft,
  constraints: RecipeConstraints,
): ErrorMap {
  const errors: ErrorMap = {};
  const limits = constraints.limits;

  // ---- Top-level scalar fields ---------------------------------------------
  const title = draft.title.trim();
  if (!title) errors["title"] = "A title is required.";
  else if (byteLength(title) > limits.max_title_bytes)
    errors["title"] = `Title must be at most ${limits.max_title_bytes} bytes.`;

  if (byteLength(draft.summary) > limits.max_text_bytes)
    errors["summary"] = `Summary is too long.`;
  if (byteLength(draft.notes) > limits.max_text_bytes)
    errors["notes"] = `Notes are too long.`;
  if (byteLength(draft.attribution) > limits.max_text_bytes)
    errors["attribution"] = `Attribution is too long.`;

  if (!LANGUAGES.some((l) => l.tag === draft.language))
    errors["language"] = "Pick a language.";
  else {
    const len = byteLength(draft.language);
    if (
      len < limits.language_tag_min_bytes ||
      len > limits.language_tag_max_bytes
    )
      errors["language"] = "Language tag is out of range.";
  }
  if (!LICENSES.some((l) => l.id === draft.license))
    errors["license"] = "Pick a license.";

  if (draft.version.trim()) {
    const v = Number(draft.version);
    if (!Number.isInteger(v) || v < 0)
      errors["version"] = "Version must be a non-negative integer.";
  }

  // ---- Yield ----------------------------------------------------------------
  const yieldAmount = draft.yieldAmount.trim();
  if (!yieldAmount && !draft.yieldUnit.trim())
    errors["yield"] = "Give an amount or a unit.";
  if (byteLength(draft.yieldUnit) > limits.max_text_bytes)
    errors["yield"] = "Yield unit is too long.";

  // ---- Times (named-phase durations) ---------------------------------------
  draft.times.forEach((t, i) => {
    if (!t.phase.trim() && !t.amount.trim()) return; // blank row — ignored
    if (!t.phase.trim()) errors[`times.${i}.phase`] = "Name this phase.";
    // D4 mirror: a quantity must carry a numeric amount or text.
    if (!t.amount.trim())
      errors[`times.${i}.amount`] = "Give a duration or remove the row.";
    if (byteLength(t.unit) > limits.unit_code_max_bytes)
      errors[`times.${i}.unit`] = "Unit is too long.";
  });

  // ---- Ingredient groups ---------------------------------------------------
  if (draft.ingredientGroups.length > limits.max_ingredient_groups)
    errors["ingredient_groups"] = `Too many groups.`;
  draft.ingredientGroups.forEach((g, i) => {
    if (!g.trim()) errors[`group.${i}`] = "Group name cannot be empty.";
    else if (byteLength(g) > limits.max_text_bytes)
      errors[`group.${i}`] = "Group name is too long.";
  });

  // ---- Ingredients ---------------------------------------------------------
  const filledIngredients = draft.ingredients.filter((row) => row.name.trim());
  if (filledIngredients.length === 0)
    errors["ingredients"] = "Add at least one ingredient.";
  if (draft.ingredients.length > limits.max_ingredients)
    errors["ingredients"] = `Too many ingredients.`;

  draft.ingredients.forEach((row, i) => {
    if (!row.name.trim()) {
      // Empty rows are allowed (they are filtered at build time) — except
      // the row is invalid if other fields on it are filled.
      if (
        row.amount.trim() ||
        row.unit.trim() ||
        row.display.trim() ||
        row.preparation.trim() ||
        row.substitutes.some((s) => s.name.trim())
      )
        errors[`ingredients.${i}.name`] = "Name this ingredient.";
      return;
    }
    if (byteLength(row.name) > limits.max_text_bytes)
      errors[`ingredients.${i}.name`] = "Name is too long.";
    if (byteLength(row.display) > limits.max_text_bytes)
      errors[`ingredients.${i}.display`] = "Display amount is too long.";
    if (byteLength(row.preparation) > limits.max_text_bytes)
      errors[`ingredients.${i}.preparation`] = "Preparation note is too long.";
    if (byteLength(row.unit) > limits.unit_code_max_bytes)
      errors[`ingredients.${i}.unit`] = "Unit is too long.";

    if (row.group !== null) {
      if (row.group < 0 || row.group >= draft.ingredientGroups.length)
        errors[`ingredients.${i}.group`] = "Group is out of range.";
    }

    if (row.substitutes.length > limits.max_substitutes)
      errors[`ingredients.${i}.substitutes`] = "Too many substitutes.";
    row.substitutes.forEach((s, j) => {
      if (!s.name.trim() && !s.amount.trim()) return; // blank row — ignored
      if (!s.name.trim())
        errors[`ingredients.${i}.substitutes.${j}.name`] =
          "Name this substitute.";
      if (byteLength(s.name) > limits.max_text_bytes)
        errors[`ingredients.${i}.substitutes.${j}.name`] = "Name is too long.";
      if (byteLength(s.unit) > limits.unit_code_max_bytes)
        errors[`ingredients.${i}.substitutes.${j}.unit`] = "Unit is too long.";
    });
  });

  // ---- Equipment -----------------------------------------------------------
  if (draft.equipment.length > limits.max_equipment)
    errors["equipment"] = `Too many equipment items.`;
  draft.equipment.forEach((row, i) => {
    if (!row.name.trim()) {
      if (row.note.trim())
        errors[`equipment.${i}.name`] = "Name this equipment.";
      return;
    }
    if (byteLength(row.name) > limits.max_text_bytes)
      errors[`equipment.${i}.name`] = "Name is too long.";
    if (byteLength(row.note) > limits.max_text_bytes)
      errors[`equipment.${i}.note`] = "Note is too long.";
  });

  // ---- Steps ---------------------------------------------------------------
  const filledSteps = draft.steps.filter((s) => s.text.trim());
  if (filledSteps.length === 0) errors["steps"] = "Add at least one step.";
  if (draft.steps.length > limits.max_steps)
    errors["steps"] = "Too many steps.";

  const ingredientIds = new Set(draft.ingredients.map((row) => row.id));
  const equipmentIds = new Set(draft.equipment.map((row) => row.id));

  draft.steps.forEach((row, i) => {
    if (!row.text.trim()) {
      if (
        row.group.trim() ||
        row.durationAmount.trim() ||
        row.temperatureValue.trim() ||
        row.produces.trim() ||
        row.ingredientIds.length > 0 ||
        row.equipmentIds.length > 0
      )
        errors[`steps.${i}.text`] = "Write this step.";
      return;
    }
    if (byteLength(row.text) > limits.max_text_bytes)
      errors[`steps.${i}.text`] = "Step text is too long.";
    if (byteLength(row.group) > limits.max_text_bytes)
      errors[`steps.${i}.group`] = "Step group name is too long.";
    if (byteLength(row.produces) > limits.max_text_bytes)
      errors[`steps.${i}.produces`] = "Produces text is too long.";

    // Duration: D4 mirror — give an amount, or leave both blank.
    if (row.durationAmount.trim()) {
      if (byteLength(row.durationUnit) > limits.unit_code_max_bytes)
        errors[`steps.${i}.durationUnit`] = "Unit is too long.";
    } else if (row.durationUnit.trim()) {
      errors[`steps.${i}.durationAmount`] =
        "Give a duration or clear the unit.";
    }

    // Temperature: value and unit go together.
    const tv = row.temperatureValue.trim();
    const tu = row.temperatureUnit.trim();
    if (tv && !tu)
      errors[`steps.${i}.temperatureUnit`] = "Pick a temperature unit.";
    if (!tv && tu)
      errors[`steps.${i}.temperatureValue`] = "Give a temperature value.";
    if (tv && !Number.isFinite(Number(tv)))
      errors[`steps.${i}.temperatureValue`] = "Temperature must be numeric.";
    if (byteLength(row.temperatureUnit) > limits.unit_code_max_bytes)
      errors[`steps.${i}.temperatureUnit`] = "Unit is too long.";

    // Refs: every selected id must still exist.
    if (row.ingredientIds.length > limits.max_step_ingredient_refs)
      errors[`steps.${i}.ingredientIds`] = "Too many ingredient references.";
    for (const id of row.ingredientIds) {
      if (!ingredientIds.has(id)) {
        errors[`steps.${i}.ingredientIds`] =
          "An ingredient reference is stale.";
        break;
      }
    }
    if (row.equipmentIds.length > limits.max_step_equipment_refs)
      errors[`steps.${i}.equipmentIds`] = "Too many equipment references.";
    for (const id of row.equipmentIds) {
      if (!equipmentIds.has(id)) {
        errors[`steps.${i}.equipmentIds`] = "An equipment reference is stale.";
        break;
      }
    }
  });

  // ---- Tag-like comma lists ------------------------------------------------
  const tagList = splitCommas(draft.tags);
  if (tagList.length > limits.max_tags) errors["tags"] = "Too many tags.";
  if (tagList.some((t) => byteLength(t) > limits.max_text_bytes))
    errors["tags"] = "A tag is too long.";

  const cuisineList = splitCommas(draft.cuisine);
  if (cuisineList.length > limits.max_cuisine)
    errors["cuisine"] = "Too many cuisine tags.";
  if (cuisineList.some((t) => byteLength(t) > limits.max_text_bytes))
    errors["cuisine"] = "A cuisine tag is too long.";

  // ---- Enum classification arrays ------------------------------------------
  if (draft.course.length > limits.max_course) errors["course"] = "Too many.";
  if (draft.diet.length > limits.max_diet) errors["diet"] = "Too many.";
  if (draft.allergens.length > limits.max_allergens)
    errors["allergens"] = "Too many.";
  for (const c of draft.course)
    if (!constraints.enums.course.includes(c)) errors["course"] = "Bad value.";
  for (const c of draft.diet)
    if (!constraints.enums.diet.includes(c)) errors["diet"] = "Bad value.";
  for (const c of draft.allergens)
    if (!constraints.enums.allergens.includes(c))
      errors["allergens"] = "Bad value.";

  // ---- Nutrition -----------------------------------------------------------
  const n = draft.nutrition;
  const nutritionFilled =
    n.basisAmount.trim() ||
    n.basisUnit.trim() ||
    Object.values(n.values).some((v) => v.amount.trim());
  if (nutritionFilled) {
    if (!n.basisAmount.trim())
      errors["nutrition.basisAmount"] =
        "Give a basis amount or clear the nutrition block.";
    if (byteLength(n.basisUnit) > limits.unit_code_max_bytes)
      errors["nutrition.basisUnit"] = "Unit is too long.";
    for (const code of Object.keys(n.values)) {
      if (!constraints.enums.nutrients.includes(code))
        errors[`nutrition.values.${code}`] = "Unknown nutrient.";
      const v = n.values[code];
      if (!v) continue;
      if (v.amount.trim() && !Number.isFinite(Number(v.amount)))
        errors[`nutrition.values.${code}`] = "Amount must be numeric.";
      if (byteLength(v.unit) > limits.unit_code_max_bytes)
        errors[`nutrition.values.${code}`] = "Unit is too long.";
    }
  }

  return errors;
}

/** Splits a comma-separated string into trimmed non-empty entries. */
export function splitCommas(raw: string): string[] {
  return raw
    .split(",")
    .map((t) => t.trim())
    .filter(Boolean);
}
