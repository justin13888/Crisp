/**
 * Pure editing operations on a {@link FormState}: allocating stable refs,
 * and adding, inserting, duplicating, moving, and deleting ingredients,
 * steps, equipment, and sections. Each returns a new state (never mutates).
 */

import type { BulkSection } from "./bulk";
import {
  blankEquipment,
  blankIngredient,
  blankStep,
  type EquipmentForm,
  type FormState,
  type GroupForm,
  type IngredientForm,
  LIMITS,
  type StepForm,
} from "./model";

/** Which section list an operation targets. */
export type GroupKind = "ingredient" | "step";

/** A move direction: earlier (-1) or later (+1). */
export type Direction = -1 | 1;

/** A new state plus the ref (or key) the operation created. */
export interface Created {
  /** The new state. */
  form: FormState;
  /** The created item's ref or key. */
  ref: number;
}

/** Allocates `count` fresh refs; refs are never reused within a draft. */
export function allocRefs(
  form: FormState,
  count = 1,
): { form: FormState; refs: number[] } {
  const refs = Array.from({ length: count }, (_, i) => form.nextRef + i);
  return { form: { ...form, nextRef: form.nextRef + count }, refs };
}

function allocRef(form: FormState): { form: FormState; ref: number } {
  const { form: next, refs } = allocRefs(form, 1);
  return { form: next, ref: refs[0] ?? form.nextRef };
}

/** Swaps `item` with its nearest neighbor in `dir` that `sameLane` accepts. */
export function moveWithin<T>(
  list: readonly T[],
  index: number,
  dir: Direction,
  sameLane: (other: T) => boolean = () => true,
): T[] {
  let j = index + dir;
  while (j >= 0 && j < list.length) {
    const other = list[j];
    if (other !== undefined && sameLane(other)) break;
    j += dir;
  }
  const a = list[index];
  const b = list[j];
  if (a === undefined || b === undefined) return [...list];
  const next = [...list];
  next[index] = b;
  next[j] = a;
  return next;
}

function insertAt<T>(list: readonly T[], index: number, item: T): T[] {
  const next = [...list];
  next.splice(Math.max(0, Math.min(index, list.length)), 0, item);
  return next;
}

/** The position after `after` (by ref), or the end of the list. */
function afterIndex<T extends { ref: number }>(
  list: readonly T[],
  after: number | undefined,
): number {
  if (after === undefined) return list.length;
  const i = list.findIndex((item) => item.ref === after);
  return i < 0 ? list.length : i + 1;
}

/** The position after the last item of `group`, or the end of the list. */
function endOfGroup<T extends { group: number | null }>(
  list: readonly T[],
  group: number | null,
): number {
  for (let i = list.length - 1; i >= 0; i--) {
    if (list[i]?.group === group) return i + 1;
  }
  return list.length;
}

// ------------------------------------------------------------ ingredients

/** Adds a blank ingredient after `after` (or at the end of `group`). */
export function addIngredient(
  form: FormState,
  opts: { after?: number; group?: number | null } = {},
): Created {
  const { form: next, ref } = allocRef(form);
  const anchor = next.ingredients.find((i) => i.ref === opts.after);
  const group = opts.group !== undefined ? opts.group : (anchor?.group ?? null);
  const at = anchor
    ? afterIndex(next.ingredients, opts.after)
    : endOfGroup(next.ingredients, group);
  return {
    form: {
      ...next,
      ingredients: insertAt(next.ingredients, at, blankIngredient(ref, group)),
    },
    ref,
  };
}

/** Copies an ingredient (under a new ref) right after it. */
export function duplicateIngredient(form: FormState, ref: number): Created {
  const source = form.ingredients.find((i) => i.ref === ref);
  if (!source) return { form, ref };
  const { form: next, ref: copyRef } = allocRef(form);
  const { orig: _orig, ...rest } = source;
  const copy: IngredientForm = {
    ...rest,
    ref: copyRef,
    alt: [...source.alt],
    substitutes: source.substitutes.map(({ orig: _o, ...s }) => ({ ...s })),
  };
  return {
    form: {
      ...next,
      ingredients: insertAt(
        next.ingredients,
        afterIndex(next.ingredients, ref),
        copy,
      ),
    },
    ref: copyRef,
  };
}

/** Deletes an ingredient and every step's use of it. */
export function removeIngredient(form: FormState, ref: number): FormState {
  return {
    ...form,
    ingredients: form.ingredients.filter((i) => i.ref !== ref),
    steps: form.steps.map((step) =>
      step.ingredients.some((p) => p.ref === ref)
        ? {
            ...step,
            ingredients: step.ingredients.filter((p) => p.ref !== ref),
          }
        : step,
    ),
  };
}

/** Moves an ingredient up or down within its section. */
export function moveIngredient(
  form: FormState,
  ref: number,
  dir: Direction,
): FormState {
  const index = form.ingredients.findIndex((i) => i.ref === ref);
  const item = form.ingredients[index];
  if (!item) return form;
  return {
    ...form,
    ingredients: moveWithin(
      form.ingredients,
      index,
      dir,
      (other) => other.group === item.group,
    ),
  };
}

/** Applies a change to one ingredient. */
export function updateIngredient(
  form: FormState,
  ref: number,
  patch: Partial<Omit<IngredientForm, "ref">>,
): FormState {
  return {
    ...form,
    ingredients: form.ingredients.map((i) =>
      i.ref === ref ? { ...i, ...patch } : i,
    ),
  };
}

/**
 * Appends pasted ingredients: titled sections become new ingredient
 * groups (or join an existing one with the same name); untitled lines go
 * into `group`. Replaces a lone blank row so pasting into a new recipe
 * does not leave an empty first line.
 */
export function addBulkIngredients(
  form: FormState,
  sections: readonly BulkSection[],
  group: number | null = null,
): FormState {
  let next: FormState = {
    ...form,
    ingredients: form.ingredients.filter(
      (i) =>
        i.name.trim() ||
        i.amount.trim() ||
        i.preparation.trim() ||
        i.note.trim() ||
        i.orig ||
        form.steps.some((s) => s.ingredients.some((p) => p.ref === i.ref)),
    ),
  };
  for (const section of sections) {
    let target = group;
    if (section.title !== null) {
      const title = section.title;
      const existing = next.ingredientGroups.find(
        (g) => g.name.trim().toLowerCase() === title.toLowerCase(),
      );
      if (existing) target = existing.key;
      else {
        const created = addGroup(next, "ingredient", title);
        next = created.form;
        target = created.ref;
      }
    }
    const { form: allocated, refs } = allocRefs(next, section.items.length);
    next = allocated;
    const rows = section.items.map((item, i) => ({
      ...blankIngredient(refs[i] ?? 0, target),
      amount: item.amount,
      name: item.name,
      preparation: item.preparation,
    }));
    const at = endOfGroup(next.ingredients, target);
    const ingredients = [...next.ingredients];
    ingredients.splice(at, 0, ...rows);
    next = { ...next, ingredients };
  }
  return next;
}

// ------------------------------------------------------------------ steps

/** Adds a blank step after `after` (or at the end of `group`). */
export function addStep(
  form: FormState,
  opts: { after?: number; group?: number | null } = {},
): Created {
  const { form: next, ref } = allocRef(form);
  const anchor = next.steps.find((s) => s.ref === opts.after);
  const group = opts.group !== undefined ? opts.group : (anchor?.group ?? null);
  const at = anchor
    ? afterIndex(next.steps, opts.after)
    : endOfGroup(next.steps, group);
  const unit =
    [...next.steps].reverse().find((s) => s.tempMin.trim())?.tempUnit ??
    next.steps.at(-1)?.tempUnit ??
    "Cel";
  return {
    form: {
      ...next,
      steps: insertAt(next.steps, at, blankStep(ref, group, unit)),
    },
    ref,
  };
}

/** Copies a step (under a new ref) right after it. */
export function duplicateStep(form: FormState, ref: number): Created {
  const source = form.steps.find((s) => s.ref === ref);
  if (!source) return { form, ref };
  const { form: next, ref: copyRef } = allocRef(form);
  const { orig: _orig, ...rest } = source;
  const copy: StepForm = {
    ...rest,
    ref: copyRef,
    ingredients: source.ingredients.map((p) => ({ ...p })),
    equipment: [...source.equipment],
    timers: source.timers.map((t) => ({ ...t })),
  };
  return {
    form: {
      ...next,
      steps: insertAt(next.steps, afterIndex(next.steps, ref), copy),
    },
    ref: copyRef,
  };
}

/** Deletes a step. */
export function removeStep(form: FormState, ref: number): FormState {
  return { ...form, steps: form.steps.filter((s) => s.ref !== ref) };
}

/** Moves a step up or down within its section. */
export function moveStep(
  form: FormState,
  ref: number,
  dir: Direction,
): FormState {
  const index = form.steps.findIndex((s) => s.ref === ref);
  const step = form.steps[index];
  if (!step) return form;
  return {
    ...form,
    steps: moveWithin(
      form.steps,
      index,
      dir,
      (other) => other.group === step.group,
    ),
  };
}

/** Applies a change to one step. */
export function updateStep(
  form: FormState,
  ref: number,
  patch: Partial<Omit<StepForm, "ref">>,
): FormState {
  return {
    ...form,
    steps: form.steps.map((s) => (s.ref === ref ? { ...s, ...patch } : s)),
  };
}

/** Toggles whether a step uses an ingredient. */
export function toggleStepIngredient(
  form: FormState,
  stepRef: number,
  ingredientRef: number,
): FormState {
  const step = form.steps.find((s) => s.ref === stepRef);
  if (!step) return form;
  const used = step.ingredients.some((p) => p.ref === ingredientRef);
  return updateStep(form, stepRef, {
    ingredients: used
      ? step.ingredients.filter((p) => p.ref !== ingredientRef)
      : [...step.ingredients, { ref: ingredientRef, amount: "" }],
  });
}

/** Toggles whether a step uses a piece of equipment. */
export function toggleStepEquipment(
  form: FormState,
  stepRef: number,
  equipmentRef: number,
): FormState {
  const step = form.steps.find((s) => s.ref === stepRef);
  if (!step) return form;
  const used = step.equipment.includes(equipmentRef);
  return updateStep(form, stepRef, {
    equipment: used
      ? step.equipment.filter((r) => r !== equipmentRef)
      : [...step.equipment, equipmentRef],
  });
}

/** Appends pasted steps (one per text) to `group`, replacing a lone blank step. */
export function addBulkSteps(
  form: FormState,
  texts: readonly string[],
  group: number | null = null,
): FormState {
  const kept = form.steps.filter(
    (s) =>
      s.text.trim() ||
      s.ingredients.length ||
      s.timers.length ||
      s.tempMin.trim() ||
      s.note.trim() ||
      s.image ||
      s.orig,
  );
  const { form: next, refs } = allocRefs(
    { ...form, steps: kept },
    texts.length,
  );
  const unit = form.steps.at(-1)?.tempUnit ?? "Cel";
  const rows = texts.map((text, i) => ({
    ...blankStep(refs[i] ?? 0, group, unit),
    text,
  }));
  const steps = [...next.steps];
  steps.splice(endOfGroup(steps, group), 0, ...rows);
  return { ...next, steps };
}

// -------------------------------------------------------------- equipment

/** Adds a blank equipment row after `after` (or at the end). */
export function addEquipment(
  form: FormState,
  opts: { after?: number } = {},
): Created {
  const { form: next, ref } = allocRef(form);
  return {
    form: {
      ...next,
      equipment: insertAt(
        next.equipment,
        afterIndex(next.equipment, opts.after),
        blankEquipment(ref),
      ),
    },
    ref,
  };
}

/** Copies an equipment row (under a new ref) right after it. */
export function duplicateEquipment(form: FormState, ref: number): Created {
  const source = form.equipment.find((e) => e.ref === ref);
  if (!source) return { form, ref };
  const { form: next, ref: copyRef } = allocRef(form);
  const { orig: _orig, ...rest } = source;
  const copy: EquipmentForm = { ...rest, ref: copyRef };
  return {
    form: {
      ...next,
      equipment: insertAt(
        next.equipment,
        afterIndex(next.equipment, ref),
        copy,
      ),
    },
    ref: copyRef,
  };
}

/** Deletes equipment and every step's use of it. */
export function removeEquipment(form: FormState, ref: number): FormState {
  return {
    ...form,
    equipment: form.equipment.filter((e) => e.ref !== ref),
    steps: form.steps.map((step) =>
      step.equipment.includes(ref)
        ? { ...step, equipment: step.equipment.filter((r) => r !== ref) }
        : step,
    ),
  };
}

/** Moves equipment up or down. */
export function moveEquipment(
  form: FormState,
  ref: number,
  dir: Direction,
): FormState {
  const index = form.equipment.findIndex((e) => e.ref === ref);
  if (index < 0) return form;
  return { ...form, equipment: moveWithin(form.equipment, index, dir) };
}

/** Applies a change to one equipment row. */
export function updateEquipment(
  form: FormState,
  ref: number,
  patch: Partial<Omit<EquipmentForm, "ref">>,
): FormState {
  return {
    ...form,
    equipment: form.equipment.map((e) =>
      e.ref === ref ? { ...e, ...patch } : e,
    ),
  };
}

// --------------------------------------------------------------- sections

function groupsKey(kind: GroupKind) {
  return kind === "ingredient" ? "ingredientGroups" : "stepGroups";
}

/** Adds a section; returns its key. Refuses past the format's limit. */
export function addGroup(form: FormState, kind: GroupKind, name = ""): Created {
  const key = groupsKey(kind);
  if (form[key].length >= LIMITS.groups) return { form, ref: -1 };
  const { form: next, ref } = allocRef(form);
  const group: GroupForm = { key: ref, name };
  return { form: { ...next, [key]: [...next[key], group] }, ref };
}

/** Renames a section. */
export function renameGroup(
  form: FormState,
  kind: GroupKind,
  groupKey: number,
  name: string,
): FormState {
  const key = groupsKey(kind);
  return {
    ...form,
    [key]: form[key].map((g) => (g.key === groupKey ? { ...g, name } : g)),
  };
}

/** Deletes a section; its items stay, ungrouped. */
export function removeGroup(
  form: FormState,
  kind: GroupKind,
  groupKey: number,
): FormState {
  const key = groupsKey(kind);
  const groups = form[key].filter((g) => g.key !== groupKey);
  if (kind === "ingredient") {
    return {
      ...form,
      ingredientGroups: groups,
      ingredients: form.ingredients.map((i) =>
        i.group === groupKey ? { ...i, group: null } : i,
      ),
    };
  }
  return {
    ...form,
    stepGroups: groups,
    steps: form.steps.map((s) =>
      s.group === groupKey ? { ...s, group: null } : s,
    ),
  };
}

/** Moves a section earlier or later. */
export function moveGroup(
  form: FormState,
  kind: GroupKind,
  groupKey: number,
  dir: Direction,
): FormState {
  const key = groupsKey(kind);
  const index = form[key].findIndex((g) => g.key === groupKey);
  if (index < 0) return form;
  return { ...form, [key]: moveWithin(form[key], index, dir) };
}

/** Moves an item into another section (appended at its end). */
export function regroup(
  form: FormState,
  kind: GroupKind,
  ref: number,
  group: number | null,
): FormState {
  if (kind === "ingredient") {
    const item = form.ingredients.find((i) => i.ref === ref);
    if (!item || item.group === group) return form;
    const rest = form.ingredients.filter((i) => i.ref !== ref);
    return {
      ...form,
      ingredients: insertAt(rest, endOfGroup(rest, group), { ...item, group }),
    };
  }
  const step = form.steps.find((s) => s.ref === ref);
  if (!step || step.group === group) return form;
  const rest = form.steps.filter((s) => s.ref !== ref);
  return {
    ...form,
    steps: insertAt(rest, endOfGroup(rest, group), { ...step, group }),
  };
}

// ----------------------------------------------------------------- photos

/** Moves a recipe photo earlier or later (the first is the cover). */
export function moveMedia(
  form: FormState,
  id: string,
  dir: Direction,
): FormState {
  const index = form.media.indexOf(id);
  if (index < 0) return form;
  return { ...form, media: moveWithin(form.media, index, dir) };
}
