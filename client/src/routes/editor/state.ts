/**
 * Draft shapes for the recipe authoring form.
 *
 * The form authors a single language: localized values are stored as plain
 * strings here and wrapped into `{ [lang]: value }` only at build time. Every
 * row also carries a stable client-side `id` so that step references survive
 * reorders and deletions of the rows they point at.
 */

/** A monotonic id source for client-side row identity. */
let nextId = 1;

/** Allocates a new stable row id, unique within this session. */
export function newId(): number {
  return nextId++;
}

/** One ingredient row in the draft. */
export interface IngredientDraft {
  id: number;
  name: string;
  amount: string;
  unit: string;
  display: string;
  preparation: string;
  optional: boolean;
  /** Index into `RecipeDraft.ingredientGroups`, or `null` for ungrouped. */
  group: number | null;
  substitutes: SubstituteDraft[];
}

/** One substitute on an ingredient. */
export interface SubstituteDraft {
  id: number;
  name: string;
  amount: string;
  unit: string;
}

/** One step row in the draft. */
export interface StepDraft {
  id: number;
  text: string;
  group: string;
  durationAmount: string;
  durationUnit: string;
  temperatureValue: string;
  temperatureUnit: string;
  produces: string;
  /** Ids of ingredient rows this step uses. */
  ingredientIds: number[];
  /** Ids of equipment rows this step uses. */
  equipmentIds: number[];
}

/** One equipment row in the draft. */
export interface EquipmentDraft {
  id: number;
  name: string;
  note: string;
}

/** One named-phase duration row in the draft. */
export interface TimeDraft {
  id: number;
  phase: string;
  amount: string;
  unit: string;
}

/** One named-nutrient row in the draft, keyed by wire code. */
export interface NutritionValueDraft {
  amount: string;
  unit: string;
}

/** The nutrition block of the draft. */
export interface NutritionDraft {
  basisAmount: string;
  basisUnit: string;
  /** Keyed by the wire code from `RecipeConstraints.enums.nutrients`. */
  values: Record<string, NutritionValueDraft>;
}

/** The full authoring draft for a single recipe bundle. */
export interface RecipeDraft {
  title: string;
  language: string;
  license: string;
  /** Optional monotonic version counter as a string (blank means none). */
  version: string;
  summary: string;
  notes: string;
  attribution: string;
  yieldAmount: string;
  yieldUnit: string;
  times: TimeDraft[];
  ingredientGroups: string[];
  ingredients: IngredientDraft[];
  equipment: EquipmentDraft[];
  steps: StepDraft[];
  /** Comma-separated language-neutral tag list. */
  tags: string;
  /** Comma-separated language-neutral cuisine list. */
  cuisine: string;
  /** Wire codes selected from `RecipeConstraints.enums.course`. */
  course: string[];
  /** Wire codes selected from `RecipeConstraints.enums.diet`. */
  diet: string[];
  /** Wire codes selected from `RecipeConstraints.enums.allergens`. */
  allergens: string[];
  nutrition: NutritionDraft;
  publish: boolean;
}

/**
 * The starting draft a fresh `/new` form opens with. Most fields are blank;
 * only the safe defaults that match the previous form's behaviour are filled
 * in (English, a permissive license, a single empty ingredient/step row).
 */
export function emptyDraft(): RecipeDraft {
  return {
    title: "",
    language: "en",
    license: "CC-BY-4.0",
    version: "",
    summary: "",
    notes: "",
    attribution: "",
    yieldAmount: "4",
    yieldUnit: "servings",
    times: [],
    ingredientGroups: [],
    ingredients: [emptyIngredient()],
    equipment: [],
    steps: [emptyStep()],
    tags: "",
    cuisine: "",
    course: [],
    diet: [],
    allergens: [],
    nutrition: { basisAmount: "", basisUnit: "", values: {} },
    publish: false,
  };
}

/** A blank ingredient row, ready to be appended to a draft. */
export function emptyIngredient(): IngredientDraft {
  return {
    id: newId(),
    name: "",
    amount: "",
    unit: "",
    display: "",
    preparation: "",
    optional: false,
    group: null,
    substitutes: [],
  };
}

/** A blank substitute row. */
export function emptySubstitute(): SubstituteDraft {
  return { id: newId(), name: "", amount: "", unit: "" };
}

/** A blank step row. */
export function emptyStep(): StepDraft {
  return {
    id: newId(),
    text: "",
    group: "",
    durationAmount: "",
    durationUnit: "",
    temperatureValue: "",
    temperatureUnit: "",
    produces: "",
    ingredientIds: [],
    equipmentIds: [],
  };
}

/** A blank equipment row. */
export function emptyEquipment(): EquipmentDraft {
  return { id: newId(), name: "", note: "" };
}

/** A blank named-phase duration row. */
export function emptyTime(): TimeDraft {
  return { id: newId(), phase: "", amount: "", unit: "" };
}
