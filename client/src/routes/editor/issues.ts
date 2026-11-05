/**
 * Mapping engine validation issues (JSON Pointers into the wire draft) back
 * onto the form fields that rendered them, and the field labels (in the
 * interface language) that problems carry.
 */

import type { Issue } from "../../generated/crisp";
import { i18n } from "../../i18n";
import type { DraftIndex } from "./convert";
import {
  type FieldProblem,
  fieldId,
  type IngredientPart,
  type StepPart,
  TIME_KEYS,
  type TimeKey,
} from "./model";

/** Where a pointer lands: a field id and a human label. */
export interface FieldLocation {
  /** The field's DOM id, or `null` when no single field fits. */
  field: string | null;
  /** A human label for the summary list. */
  label: string;
}

/** A part of a list item, named after the item ("Step 2 · heat"). */
export type ItemPart =
  | "amount"
  | "preparation"
  | "note"
  | "quantity"
  | "measures"
  | "ingredients"
  | "timers"
  | "temperature"
  | "temperatureMax"
  | "group"
  | "equipment"
  | "heat"
  | "produces"
  | "image";

/** A numbered part of a list item ("Step 2 · timer 1"). */
export type NumberedPart = "measure" | "substitute" | "ingredient" | "timer";

/** A whole-form field with a fixed label. */
export type FormField =
  | "title"
  | "summary"
  | "yield"
  | "yieldUnit"
  | "times"
  | "ingredients"
  | "equipment"
  | "steps"
  | "tags"
  | "cuisine"
  | "course"
  | "diet"
  | "allergens"
  | "bestMonths"
  | "photos"
  | "notes"
  | "license";

/** A list item by zero-based position: "Ingredient 3". */
export function itemLabel(
  kind: "ingredient" | "equipment" | "step",
  i: number,
): string {
  return i18n.t(`editor:where.${kind}`, { number: i + 1 });
}

/**
 * A part of the item `where` names: "Ingredient 3 · amount", or with a
 * zero-based `j`, "Ingredient 3 · measure 2".
 */
export function partLabel(
  where: string,
  part: ItemPart | NumberedPart,
  j = 0,
): string {
  const name = i18n.t(`editor:part.${part}`, { number: j + 1 });
  return i18n.t("editor:where.part", { where, part: name });
}

/** A whole-form field's label: "Title", "Yield unit". */
export function fieldLabel(field: FormField): string {
  return i18n.t(`editor:field.${field}`);
}

/** A whole-recipe duration's label: "Prep time". */
export function timeLabel(time: TimeKey): string {
  return i18n.t(`editor:field.time.${time}`);
}

/** No single field: the problem is the recipe's as a whole. */
function wholeRecipe(): FieldLocation {
  return { field: null, label: i18n.t("editor:problems.recipe") };
}

/** Splits an RFC 6901 JSON Pointer into unescaped segments. */
export function pointerSegments(pointer: string): string[] {
  if (!pointer || pointer === "/") return [];
  return pointer
    .replace(/^\//, "")
    .split("/")
    .map((s) => s.replace(/~1/g, "/").replace(/~0/g, "~"));
}

function position(segment: string | undefined): number | null {
  if (segment === undefined || !/^\d+$/.test(segment)) return null;
  return Number(segment);
}

const INGREDIENT_PARTS: Record<string, IngredientPart> = {
  name: "name",
  quantity: "amount",
  preparation: "preparation",
  note: "note",
};

function ingredientLocation(rest: string[], index: DraftIndex): FieldLocation {
  const i = position(rest[0]);
  const ref = i === null ? undefined : index.ingredients[i];
  if (i === null || ref === undefined) {
    return { field: fieldId.ingredients, label: fieldLabel("ingredients") };
  }
  const where = itemLabel("ingredient", i);
  const key = rest[1];
  if (key === "alt") {
    const j = position(rest[2]);
    return j === null
      ? {
          field: fieldId.ingredient(ref, "amount"),
          label: partLabel(where, "measures"),
        }
      : {
          field: fieldId.ingredientAlt(ref, j),
          label: partLabel(where, "measure", j),
        };
  }
  if (key === "substitutes") {
    const j = position(rest[2]);
    if (j === null) {
      return { field: fieldId.ingredient(ref, "name"), label: where };
    }
    const part = rest[3] === "quantity" ? "amount" : "name";
    return {
      field: fieldId.substitute(ref, j, part),
      label: partLabel(where, "substitute", j),
    };
  }
  const part = (key ? INGREDIENT_PARTS[key] : undefined) ?? "name";
  return {
    field: fieldId.ingredient(ref, part),
    label: part === "name" ? where : partLabel(where, part),
  };
}

function equipmentLocation(rest: string[], index: DraftIndex): FieldLocation {
  const i = position(rest[0]);
  const ref = i === null ? undefined : index.equipment[i];
  if (i === null || ref === undefined) {
    return { field: fieldId.equipmentList, label: fieldLabel("equipment") };
  }
  const where = itemLabel("equipment", i);
  const part =
    rest[1] === "quantity" ? "quantity" : rest[1] === "note" ? "note" : "name";
  return {
    field: fieldId.equipment(ref, part),
    label: part === "name" ? where : partLabel(where, part),
  };
}

const STEP_PARTS: Record<string, Extract<StepPart, ItemPart> | "text"> = {
  text: "text",
  group: "group",
  ingredients: "ingredients",
  equipment: "equipment",
  heat: "heat",
  produces: "produces",
  note: "note",
  blob: "image",
};

function stepLocation(rest: string[], index: DraftIndex): FieldLocation {
  const i = position(rest[0]);
  const ref = i === null ? undefined : index.steps[i];
  if (i === null || ref === undefined) {
    return { field: fieldId.steps, label: fieldLabel("steps") };
  }
  const where = itemLabel("step", i);
  const key = rest[1];
  if (key === "ingredients") {
    const j = position(rest[2]);
    return j === null
      ? {
          field: fieldId.step(ref, "ingredients"),
          label: partLabel(where, "ingredients"),
        }
      : {
          field: fieldId.stepPick(ref, j),
          label: partLabel(where, "ingredient", j),
        };
  }
  if (key === "timers") {
    const j = position(rest[2]);
    if (j === null) {
      return {
        field: fieldId.step(ref, "text"),
        label: partLabel(where, "timers"),
      };
    }
    return {
      field: fieldId.timer(ref, j, rest[3] === "label" ? "label" : "duration"),
      label: partLabel(where, "timer", j),
    };
  }
  if (key === "temperature") {
    const part =
      rest[2] === "max"
        ? "temp-max"
        : rest[2] === "target"
          ? "temp-target"
          : "temp-min";
    return {
      field: fieldId.step(ref, part),
      label: partLabel(where, "temperature"),
    };
  }
  const part = (key ? STEP_PARTS[key] : undefined) ?? "text";
  return {
    field: fieldId.step(ref, part),
    label: part === "text" ? where : partLabel(where, part),
  };
}

function groupLocation(
  rest: string[],
  keys: number[],
  kind: "ingredient" | "step",
): FieldLocation {
  const i = position(rest[0]);
  const key = i === null ? undefined : keys[i];
  const list = kind === "ingredient" ? fieldId.ingredients : fieldId.steps;
  if (i === null || key === undefined) {
    return { field: list, label: i18n.t(`editor:field.sections.${kind}`) };
  }
  return {
    field:
      kind === "ingredient"
        ? fieldId.ingredientGroup(key)
        : fieldId.stepGroup(key),
    label: i18n.t(`editor:field.section.${kind}`, { number: i + 1 }),
  };
}

/** Recipe keys whose pointer lands on one fixed field. */
const RECIPE_FIELDS: Record<string, [string, FormField]> = {
  title: [fieldId.title, "title"],
  identity: [fieldId.title, "title"],
  summary: [fieldId.summary, "summary"],
  tags: [fieldId.tags, "tags"],
  cuisine: [fieldId.cuisine, "cuisine"],
  course: [fieldId.course, "course"],
  diet: [fieldId.diet, "diet"],
  allergens: [fieldId.allergens, "allergens"],
  best_months: [fieldId.bestMonths, "bestMonths"],
  media: [fieldId.media, "photos"],
  notes: [fieldId.notes, "notes"],
};

function recipeLocation(rest: string[], index: DraftIndex): FieldLocation {
  const [key, ...tail] = rest;
  const fixed = key ? RECIPE_FIELDS[key] : undefined;
  if (fixed) return { field: fixed[0], label: fieldLabel(fixed[1]) };
  switch (key) {
    case "yield":
      return tail[0] === "unit_text"
        ? { field: fieldId.yieldUnit, label: fieldLabel("yieldUnit") }
        : { field: fieldId.yieldAmount, label: fieldLabel("yield") };
    case "times": {
      const slot = tail[0];
      if (slot && (TIME_KEYS as readonly string[]).includes(slot)) {
        const time = slot as TimeKey;
        return { field: fieldId.time(time), label: timeLabel(time) };
      }
      return { field: fieldId.time("total"), label: fieldLabel("times") };
    }
    case "ingredient_groups":
      return groupLocation(tail, index.ingredientGroups, "ingredient");
    case "step_groups":
      return groupLocation(tail, index.stepGroups, "step");
    case "ingredients":
      return ingredientLocation(tail, index);
    case "equipment":
      return equipmentLocation(tail, index);
    case "steps":
      return stepLocation(tail, index);
    case "source": {
      const part =
        tail[0] === "title" || tail[0] === "author" ? tail[0] : "url";
      return {
        field: fieldId.source(part),
        label: i18n.t(`editor:field.source.${part}`),
      };
    }
    default:
      return wholeRecipe();
  }
}

/** Maps one JSON Pointer into the draft onto the field that rendered it. */
export function locatePointer(
  pointer: string,
  index: DraftIndex,
): FieldLocation {
  const [root, ...rest] = pointerSegments(pointer);
  switch (root) {
    case "recipes":
      return recipeLocation(rest.slice(1), index);
    case "meta":
      if (rest[0] === "title") {
        return { field: fieldId.title, label: fieldLabel("title") };
      }
      if (rest[0] === "license") {
        return { field: fieldId.license, label: fieldLabel("license") };
      }
      if (rest[0] === "cover") {
        return { field: fieldId.media, label: fieldLabel("photos") };
      }
      return wholeRecipe();
    case "blobs":
      return { field: fieldId.media, label: fieldLabel("photos") };
    default:
      return wholeRecipe();
  }
}

/** Engine issues as field problems. */
export function issuesToProblems(
  issues: readonly Issue[],
  index: DraftIndex,
): FieldProblem[] {
  return issues.map((issue) => ({
    ...locatePointer(issue.path, index),
    message: issue.message,
  }));
}

/**
 * Merges problem lists, dropping exact duplicates and, per field, engine
 * messages that restate a local parse problem already shown there.
 */
export function mergeProblems(
  local: readonly FieldProblem[],
  engine: readonly FieldProblem[],
): FieldProblem[] {
  const out: FieldProblem[] = [];
  const localFields = new Set(local.map((p) => p.field));
  const seen = new Set<string>();
  for (const problem of [...local, ...engine]) {
    if (
      problem.field !== null &&
      localFields.has(problem.field) &&
      !local.includes(problem)
    ) {
      continue;
    }
    const key = `${problem.field}\u0000${problem.message}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(problem);
  }
  return out;
}

/** Messages grouped by field id. */
export function problemsByField(
  problems: readonly FieldProblem[],
): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const problem of problems) {
    if (problem.field === null) continue;
    const list = map.get(problem.field) ?? [];
    list.push(problem.message);
    map.set(problem.field, list);
  }
  return map;
}
