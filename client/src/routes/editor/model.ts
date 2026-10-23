/**
 * The editor's form model: what the recipe editor holds while the cook
 * types. It mirrors a `BundlePayload` recipe, but keeps every amount and
 * duration as the text the cook typed (the engine parses them), uses stable
 * keys for groups (the wire format uses positional indices), and carries
 * the original wire objects of an edited recipe so fields the editor does
 * not expose survive a revision untouched.
 *
 * Everything here is plain data (structured-clonable) so it can be
 * autosaved to IndexedDB as-is.
 */

import type {
  BundlePayload,
  Blob as CrispBlob,
  Equipment,
  HeatLevel,
  Identity,
  Ingredient,
  Step,
  Substitute,
  TemperatureTarget,
  TemperatureUnit,
} from "../../generated/crisp";

/** The form-state schema version (bump on incompatible change). */
export const FORM_VERSION = 1;

/** Whole-recipe duration slots. */
export const TIME_KEYS = ["prep", "cook", "rest", "total"] as const;

/** One whole-recipe duration slot. */
export type TimeKey = (typeof TIME_KEYS)[number];

/** Human labels for {@link TIME_KEYS}. */
export const TIME_LABELS: Record<TimeKey, string> = {
  prep: "Prep",
  cook: "Cook",
  rest: "Rest",
  total: "Total",
};

/** Licenses offered by the editor (SPDX ids, or `NONE`). */
export const LICENSES = [
  { id: "CC-BY-4.0", label: "CC BY 4.0 — share with credit" },
  { id: "CC-BY-SA-4.0", label: "CC BY-SA 4.0 — share alike" },
  { id: "CC0-1.0", label: "CC0 — public domain" },
  { id: "NONE", label: "All rights reserved" },
] as const;

/** The license of a new recipe. */
export const DEFAULT_LICENSE = "CC-BY-4.0";

/** Heat levels, low to high. */
export const HEAT_LEVELS: readonly HeatLevel[] = [
  "low",
  "medium-low",
  "medium",
  "medium-high",
  "high",
];

/** What a temperature applies to. */
export const TEMPERATURE_TARGETS: readonly TemperatureTarget[] = [
  "oven",
  "internal",
  "oil",
  "pan",
  "water",
];

/** Temperature units with display labels. */
export const TEMPERATURE_UNITS: Record<TemperatureUnit, string> = {
  Cel: "°C",
  "[degF]": "°F",
  K: "K",
};

/** Format limits the editor enforces in its controls. */
export const LIMITS = {
  alt: 4,
  substitutes: 16,
  timers: 8,
  groups: 32,
  media: 64,
} as const;

/** A named section of ingredients or steps. */
export interface GroupForm {
  /** Stable key (from the ref counter); never reused within a draft. */
  key: number;
  /** The section title. */
  name: string;
}

/** A substitute for an ingredient. */
export interface SubstituteForm {
  /** What to use instead. */
  name: string;
  /** Its amount as typed (may be empty). */
  amount: string;
  /** The original wire object, when loaded from a stored recipe. */
  orig?: Substitute;
}

/** One ingredient row. */
export interface IngredientForm {
  /** Stable `ref` handle. */
  ref: number;
  /** The {@link GroupForm.key} it belongs to, or `null`. */
  group: number | null;
  /** Name. */
  name: string;
  /** Amount as typed, e.g. `1 1/2 cups` (may be empty). */
  amount: string;
  /** Preparation, e.g. `finely chopped`. */
  preparation: string;
  /** Free note. */
  note: string;
  /** Optional ingredient. */
  optional: boolean;
  /** Never scaled. */
  fixed: boolean;
  /** Equivalent measures as typed (up to {@link LIMITS.alt}). */
  alt: string[];
  /** Substitutes. */
  substitutes: SubstituteForm[];
  /** The original wire object, when loaded from a stored recipe. */
  orig?: Ingredient;
}

/** One piece of equipment. */
export interface EquipmentForm {
  /** Stable `ref` handle. */
  ref: number;
  /** Name. */
  name: string;
  /** Quantity as typed (may be empty). */
  quantity: string;
  /** Note, e.g. `9x13 inch`. */
  note: string;
  /** The original wire object, when loaded from a stored recipe. */
  orig?: Equipment;
}

/** An ingredient used by a step. */
export interface StepPickForm {
  /** The ingredient's `ref`. */
  ref: number;
  /** The portion used here, as typed (empty = all of it). */
  amount: string;
}

/** A step timer. */
export interface TimerForm {
  /** Duration as typed, e.g. `20-25 min`. */
  duration: string;
  /** Label, e.g. `simmer`. */
  label: string;
  /** Hands-off waiting. */
  passive: boolean;
}

/** One step. */
export interface StepForm {
  /** Stable `ref` handle. */
  ref: number;
  /** The {@link GroupForm.key} it belongs to, or `null`. */
  group: number | null;
  /** Instructions; blank lines separate paragraphs. */
  text: string;
  /** Ingredients used. */
  ingredients: StepPickForm[];
  /** Equipment refs used. */
  equipment: number[];
  /** Timers (up to {@link LIMITS.timers}). */
  timers: TimerForm[];
  /** Heat level, or `""` for none. */
  heat: HeatLevel | "";
  /** Temperature minimum as typed (empty = no temperature). */
  tempMin: string;
  /** Temperature maximum as typed (empty = not a range). */
  tempMax: string;
  /** Temperature unit. */
  tempUnit: TemperatureUnit;
  /** What the temperature applies to, or `""`. */
  tempTarget: TemperatureTarget | "";
  /** Names the intermediate result, e.g. `the dough`. */
  produces: string;
  /** Tips and warnings; blank lines separate paragraphs. */
  note: string;
  /** Step photo blob id, or `null`. */
  image: string | null;
  /** The original wire object, when loaded from a stored recipe. */
  orig?: Step;
}

/** Where the recipe came from. */
export interface SourceForm {
  /** Book or site name. */
  title: string;
  /** Original author. */
  author: string;
  /** An `https` URL. */
  url: string;
}

/** The whole editor state. */
export interface FormState {
  /** Schema version ({@link FORM_VERSION}). */
  v: typeof FORM_VERSION;
  /** The language being edited (the bundle's default language). */
  lang: string;
  /** The payload of the stored recipe being revised, or `null` when new. */
  base: BundlePayload | null;
  /** The recipe identity (blank birth fields are filled by `seal`). */
  identity: Identity;
  /** Title. */
  title: string;
  /** Summary; blank lines separate paragraphs. */
  summary: string;
  /** Yield amount as typed, e.g. `24` or `4-6`. */
  yieldAmount: string;
  /** Yield unit text, e.g. `cookies`. */
  yieldUnit: string;
  /** Durations as typed, by slot. */
  times: Record<TimeKey, string>;
  /** Ingredient sections, in display order. */
  ingredientGroups: GroupForm[];
  /** Ingredients (order matters within a group). */
  ingredients: IngredientForm[];
  /** Equipment. */
  equipment: EquipmentForm[];
  /** Step sections, in display order. */
  stepGroups: GroupForm[];
  /** Steps (order matters within a group). */
  steps: StepForm[];
  /** Recipe photo blob ids; the first is the cover. */
  media: string[];
  /** Every image blob the draft may reference, by id. */
  blobs: Record<string, CrispBlob>;
  /** Course codes. */
  course: string[];
  /** Diet codes. */
  diet: string[];
  /** Allergen codes. */
  allergens: string[];
  /** Cuisine slugs. */
  cuisine: string[];
  /** Tag slugs. */
  tags: string[];
  /** Notes; blank lines separate paragraphs. */
  notes: string;
  /** Source. */
  source: SourceForm;
  /** SPDX license id or `NONE`. */
  license: string;
  /** The next unused ref/key. Monotonic: never reused within a draft. */
  nextRef: number;
}

/** A problem tied to one form field. */
export interface FieldProblem {
  /** The field's DOM id (see {@link fieldId}), or `null` if unplaceable. */
  field: string | null;
  /** Where, for the summary list, e.g. `Step 2 · timer 1`. */
  label: string;
  /** What is wrong. */
  message: string;
}

/** Parts of an ingredient row that can carry an error. */
export type IngredientPart = "name" | "amount" | "preparation" | "note";
/** Parts of an equipment row that can carry an error. */
export type EquipmentPart = "name" | "quantity" | "note";
/** Parts of a step that can carry an error. */
export type StepPart =
  | "text"
  | "group"
  | "ingredients"
  | "equipment"
  | "heat"
  | "temp-min"
  | "temp-max"
  | "temp-target"
  | "produces"
  | "note"
  | "image";

/** DOM ids of every editable field; issues are mapped onto these. */
export const fieldId = {
  title: "f-title",
  summary: "f-summary",
  yieldAmount: "f-yield-amount",
  yieldUnit: "f-yield-unit",
  time: (key: TimeKey) => `f-time-${key}`,
  ingredients: "f-ingredients",
  ingredientGroup: (key: number) => `f-ig-${key}`,
  ingredient: (ref: number, part: IngredientPart) => `f-ing-${ref}-${part}`,
  ingredientAlt: (ref: number, index: number) => `f-ing-${ref}-alt-${index}`,
  substitute: (ref: number, index: number, part: "name" | "amount") =>
    `f-ing-${ref}-sub-${index}-${part}`,
  equipmentList: "f-equipment",
  equipment: (ref: number, part: EquipmentPart) => `f-eq-${ref}-${part}`,
  steps: "f-steps",
  stepGroup: (key: number) => `f-sg-${key}`,
  step: (ref: number, part: StepPart) => `f-step-${ref}-${part}`,
  stepPick: (ref: number, index: number) => `f-step-${ref}-pick-${index}`,
  timer: (ref: number, index: number, part: "duration" | "label") =>
    `f-step-${ref}-timer-${index}-${part}`,
  media: "f-media",
  course: "f-course",
  diet: "f-diet",
  allergens: "f-allergens",
  cuisine: "f-cuisine",
  tags: "f-tags",
  notes: "f-notes",
  source: (part: keyof SourceForm) => `f-source-${part}`,
  license: "f-license",
} as const;

/** A blank ingredient row. */
export function blankIngredient(
  ref: number,
  group: number | null = null,
): IngredientForm {
  return {
    ref,
    group,
    name: "",
    amount: "",
    preparation: "",
    note: "",
    optional: false,
    fixed: false,
    alt: [],
    substitutes: [],
  };
}

/** A blank equipment row. */
export function blankEquipment(ref: number): EquipmentForm {
  return { ref, name: "", quantity: "", note: "" };
}

/** A blank step. */
export function blankStep(
  ref: number,
  group: number | null = null,
  tempUnit: TemperatureUnit = "Cel",
): StepForm {
  return {
    ref,
    group,
    text: "",
    ingredients: [],
    equipment: [],
    timers: [],
    heat: "",
    tempMin: "",
    tempMax: "",
    tempUnit,
    tempTarget: "",
    produces: "",
    note: "",
    image: null,
  };
}

/** A blank timer. */
export function blankTimer(): TimerForm {
  return { duration: "", label: "", passive: false };
}

/** Whether an ingredient row has nothing typed in it. */
export function isBlankIngredient(item: IngredientForm): boolean {
  return (
    !item.name.trim() &&
    !item.amount.trim() &&
    !item.preparation.trim() &&
    !item.note.trim() &&
    item.alt.length === 0 &&
    item.substitutes.length === 0 &&
    !item.orig
  );
}

/** Whether an equipment row has nothing typed in it. */
export function isBlankEquipment(item: EquipmentForm): boolean {
  return (
    !item.name.trim() &&
    !item.quantity.trim() &&
    !item.note.trim() &&
    !item.orig
  );
}

/** Whether a step has nothing typed or picked in it. */
export function isBlankStep(step: StepForm): boolean {
  return (
    !step.text.trim() &&
    step.ingredients.length === 0 &&
    step.equipment.length === 0 &&
    step.timers.length === 0 &&
    !step.heat &&
    !step.tempMin.trim() &&
    !step.produces.trim() &&
    !step.note.trim() &&
    !step.image &&
    !step.orig
  );
}

/** The default temperature unit for a locale (°F only for the US). */
export function defaultTempUnit(locale: string): TemperatureUnit {
  return /-(US|LR|MM)\b/i.test(locale) ? "[degF]" : "Cel";
}

/** The primary language subtag of a locale (`en-US` → `en`). */
export function primaryLanguage(locale: string | undefined): string {
  const primary = (locale ?? "").split("-")[0]?.trim().toLowerCase() ?? "";
  return /^[a-z]{2,3}$/.test(primary) ? primary : "en";
}
