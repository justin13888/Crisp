/**
 * Which form rows belong in the wire draft, and in what order: blank rows
 * are dropped and grouped rows follow their section order.
 */

import {
  type EquipmentForm,
  type FormState,
  type GroupForm,
  type IngredientForm,
  isBlankEquipment,
  isBlankIngredient,
  isBlankStep,
  type StepForm,
} from "./model";

/** Orders items so ungrouped ones come first, then by section order. */
function sortByGroup<T extends { group: number | null }>(
  items: readonly T[],
  groups: readonly GroupForm[],
): T[] {
  const rank = new Map(groups.map((g, i) => [g.key, i + 1]));
  return items
    .map((item, position) => ({ item, position }))
    .sort(
      (a, b) =>
        (a.item.group === null ? 0 : (rank.get(a.item.group) ?? 0)) -
          (b.item.group === null ? 0 : (rank.get(b.item.group) ?? 0)) ||
        a.position - b.position,
    )
    .map(({ item }) => item);
}

/** The ordered list of items that belong in the output (blank rows dropped). */
export function outputIngredients(form: FormState): IngredientForm[] {
  return sortByGroup(
    form.ingredients.filter((item) => !isBlankIngredient(item)),
    form.ingredientGroups,
  );
}

/** The ordered steps that belong in the output (blank steps dropped). */
export function outputSteps(form: FormState): StepForm[] {
  return sortByGroup(
    form.steps.filter((step) => !isBlankStep(step)),
    form.stepGroups,
  );
}

/** The equipment that belongs in the output (blank rows dropped). */
export function outputEquipment(form: FormState): EquipmentForm[] {
  return form.equipment.filter((item) => !isBlankEquipment(item));
}
