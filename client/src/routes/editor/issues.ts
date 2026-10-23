/**
 * Mapping engine validation issues (JSON Pointers into the wire draft) back
 * onto the form fields that rendered them.
 */

import type { Issue } from "../../generated/crisp";
import type { DraftIndex } from "./convert";
import {
  type FieldProblem,
  fieldId,
  type IngredientPart,
  type StepPart,
  TIME_KEYS,
  TIME_LABELS,
  type TimeKey,
} from "./model";

/** Where a pointer lands: a field id and a human label. */
export interface FieldLocation {
  /** The field's DOM id, or `null` when no single field fits. */
  field: string | null;
  /** A human label for the summary list. */
  label: string;
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
    return { field: fieldId.ingredients, label: "Ingredients" };
  }
  const where = `Ingredient ${i + 1}`;
  const key = rest[1];
  if (key === "alt") {
    const j = position(rest[2]);
    return j === null
      ? {
          field: fieldId.ingredient(ref, "amount"),
          label: `${where} · measures`,
        }
      : {
          field: fieldId.ingredientAlt(ref, j),
          label: `${where} · measure ${j + 1}`,
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
      label: `${where} · substitute ${j + 1}`,
    };
  }
  const part = (key ? INGREDIENT_PARTS[key] : undefined) ?? "name";
  return {
    field: fieldId.ingredient(ref, part),
    label: part === "name" ? where : `${where} · ${part}`,
  };
}

function equipmentLocation(rest: string[], index: DraftIndex): FieldLocation {
  const i = position(rest[0]);
  const ref = i === null ? undefined : index.equipment[i];
  if (i === null || ref === undefined) {
    return { field: fieldId.equipmentList, label: "Equipment" };
  }
  const where = `Equipment ${i + 1}`;
  const part =
    rest[1] === "quantity" ? "quantity" : rest[1] === "note" ? "note" : "name";
  return {
    field: fieldId.equipment(ref, part),
    label: part === "name" ? where : `${where} · ${part}`,
  };
}

const STEP_PARTS: Record<string, StepPart> = {
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
    return { field: fieldId.steps, label: "Steps" };
  }
  const where = `Step ${i + 1}`;
  const key = rest[1];
  if (key === "ingredients") {
    const j = position(rest[2]);
    return j === null
      ? {
          field: fieldId.step(ref, "ingredients"),
          label: `${where} · ingredients`,
        }
      : {
          field: fieldId.stepPick(ref, j),
          label: `${where} · ingredient ${j + 1}`,
        };
  }
  if (key === "timers") {
    const j = position(rest[2]);
    if (j === null) {
      return { field: fieldId.step(ref, "text"), label: `${where} · timers` };
    }
    return {
      field: fieldId.timer(ref, j, rest[3] === "label" ? "label" : "duration"),
      label: `${where} · timer ${j + 1}`,
    };
  }
  if (key === "temperature") {
    const part =
      rest[2] === "max"
        ? "temp-max"
        : rest[2] === "target"
          ? "temp-target"
          : "temp-min";
    return { field: fieldId.step(ref, part), label: `${where} · temperature` };
  }
  const part = (key ? STEP_PARTS[key] : undefined) ?? "text";
  return {
    field: fieldId.step(ref, part),
    label: part === "text" ? where : `${where} · ${part}`,
  };
}

function groupLocation(
  rest: string[],
  keys: number[],
  kind: "Ingredient" | "Step",
): FieldLocation {
  const i = position(rest[0]);
  const key = i === null ? undefined : keys[i];
  const list = kind === "Ingredient" ? fieldId.ingredients : fieldId.steps;
  if (i === null || key === undefined) {
    return { field: list, label: `${kind} sections` };
  }
  return {
    field:
      kind === "Ingredient"
        ? fieldId.ingredientGroup(key)
        : fieldId.stepGroup(key),
    label: `${kind} section ${i + 1}`,
  };
}

function recipeLocation(rest: string[], index: DraftIndex): FieldLocation {
  const [key, ...tail] = rest;
  switch (key) {
    case "title":
    case "identity":
      return { field: fieldId.title, label: "Title" };
    case "summary":
      return { field: fieldId.summary, label: "Summary" };
    case "yield":
      return tail[0] === "unit_text"
        ? { field: fieldId.yieldUnit, label: "Yield unit" }
        : { field: fieldId.yieldAmount, label: "Yield" };
    case "times": {
      const slot = tail[0];
      if (slot && (TIME_KEYS as readonly string[]).includes(slot)) {
        const time = slot as TimeKey;
        return {
          field: fieldId.time(time),
          label: `${TIME_LABELS[time]} time`,
        };
      }
      return { field: fieldId.time("total"), label: "Times" };
    }
    case "ingredient_groups":
      return groupLocation(tail, index.ingredientGroups, "Ingredient");
    case "step_groups":
      return groupLocation(tail, index.stepGroups, "Step");
    case "ingredients":
      return ingredientLocation(tail, index);
    case "equipment":
      return equipmentLocation(tail, index);
    case "steps":
      return stepLocation(tail, index);
    case "tags":
      return { field: fieldId.tags, label: "Tags" };
    case "cuisine":
      return { field: fieldId.cuisine, label: "Cuisine" };
    case "course":
      return { field: fieldId.course, label: "Course" };
    case "diet":
      return { field: fieldId.diet, label: "Diet" };
    case "allergens":
      return { field: fieldId.allergens, label: "Allergens" };
    case "media":
      return { field: fieldId.media, label: "Photos" };
    case "notes":
      return { field: fieldId.notes, label: "Notes" };
    case "source": {
      const part =
        tail[0] === "title" || tail[0] === "author" ? tail[0] : "url";
      return { field: fieldId.source(part), label: `Source ${part}` };
    }
    default:
      return { field: null, label: "Recipe" };
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
      if (rest[0] === "title") return { field: fieldId.title, label: "Title" };
      if (rest[0] === "license") {
        return { field: fieldId.license, label: "License" };
      }
      if (rest[0] === "cover") return { field: fieldId.media, label: "Photos" };
      return { field: null, label: "Recipe" };
    case "blobs":
      return { field: fieldId.media, label: "Photos" };
    default:
      return { field: null, label: "Recipe" };
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
