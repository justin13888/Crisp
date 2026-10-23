/**
 * Converting between the editor's {@link FormState} and the wire
 * `BundlePayload` draft that the engine validates and seals.
 *
 * Amount and duration parsing is the engine's job; it is injected as an
 * {@link AmountEngine} so this module stays pure and testable without WASM.
 */

import type {
  BundlePayload,
  Blob as CrispBlob,
  Equipment,
  Identity,
  Ingredient,
  LocalizedText,
  ParsedAmount,
  Quantity,
  Rational,
  Recipe,
  Source,
  Step,
  StepIngredient,
  Substitute,
  Temperature,
  TemperatureUnit,
  Times,
  Timer,
} from "../../generated/crisp";
import {
  DEFAULT_LICENSE,
  type EquipmentForm,
  FORM_VERSION,
  type FieldProblem,
  type FormState,
  fieldId,
  type GroupForm,
  type IngredientForm,
  blankIngredient,
  blankStep,
  isBlankEquipment,
  isBlankIngredient,
  isBlankStep,
  type StepForm,
  TIME_KEYS,
  TIME_LABELS,
  type TimeKey,
} from "./model";
import {
  asRecord,
  asWireMap,
  readProse,
  readText,
  requiredProse,
  requiredText,
  writeProse,
  writeText,
} from "./wire";

/** The engine functions conversion needs (a subset of the WASM module). */
export interface AmountEngine {
  /** Parses a typed amount (`"1 1/2 cups"`); throws on empty input. */
  parseAmount(text: string, lang: string): ParsedAmount;
  /** Parses a typed duration (`"1h 30m"`); throws if not a duration. */
  parseDuration(text: string): Quantity;
  /** Writes a quantity back as editable text. */
  quantityToInput(quantity: Quantity, lang: string): string;
}

/**
 * Where each wire array element came from, so engine issues (JSON Pointers
 * into the draft) can be mapped back onto form fields.
 */
export interface DraftIndex {
  /** Ingredient ref at each output position. */
  ingredients: number[];
  /** Step ref at each output position. */
  steps: number[];
  /** Equipment ref at each output position. */
  equipment: number[];
  /** Ingredient group key at each output position. */
  ingredientGroups: number[];
  /** Step group key at each output position. */
  stepGroups: number[];
}

/** The result of {@link formToDraft}. */
export interface DraftResult {
  /** The draft, ready for `validateDraft`/`seal`. */
  draft: BundlePayload;
  /** Output positions → form identities. */
  index: DraftIndex;
  /** Problems found while parsing typed amounts and durations. */
  problems: FieldProblem[];
}

/** Options for a brand-new recipe. */
export interface NewFormOptions {
  /** The language the cook writes in (primary subtag). */
  lang: string;
  /** A fresh identity (`newIdentity("", lang, 0)`): `seal` fills the rest. */
  identity: Identity;
  /** The temperature unit new steps start with. */
  tempUnit?: TemperatureUnit;
}

/** The empty form of a new recipe. */
export function newForm({
  lang,
  identity,
  tempUnit = "Cel",
}: NewFormOptions): FormState {
  return {
    v: FORM_VERSION,
    lang,
    base: null,
    identity,
    title: "",
    summary: "",
    yieldAmount: "",
    yieldUnit: "",
    times: { prep: "", cook: "", rest: "", total: "" },
    ingredientGroups: [],
    ingredients: [blankIngredient(1)],
    equipment: [],
    stepGroups: [],
    steps: [blankStep(2, null, tempUnit)],
    media: [],
    blobs: {},
    course: [],
    diet: [],
    allergens: [],
    cuisine: [],
    tags: [],
    notes: "",
    source: { title: "", author: "", url: "" },
    license: DEFAULT_LICENSE,
    nextRef: 3,
  };
}

// ---------------------------------------------------------------- loading

function input(
  engine: AmountEngine,
  quantity: Quantity | undefined,
  lang: string,
): string {
  if (!quantity) return "";
  try {
    return engine.quantityToInput(quantity, lang);
  } catch {
    return readText(quantity.text, lang);
  }
}

function rationalText([n, d]: Rational): string {
  if (d === 1) return String(n);
  const decimal = n / d;
  return Number.isInteger(decimal * 1000) ? String(decimal) : `${n}/${d}`;
}

function temperatureUnit(value: unknown): TemperatureUnit {
  return value === "[degF]" || value === "K" ? value : "Cel";
}

/**
 * The form for a stored recipe (`bundle.draft()`), editing its default
 * language. Fields the editor does not expose are kept in `base`/`orig`.
 */
export function draftToForm(
  payload: BundlePayload,
  engine: AmountEngine,
): FormState {
  const lang = payload.default_language;
  const recipe = payload.recipes[0];
  if (!recipe) throw new Error("This file has no recipe to edit.");

  let maxRef = 0;
  const seen = (ref: number) => {
    maxRef = Math.max(maxRef, ref);
  };
  for (const item of recipe.ingredients) seen(item.ref);
  for (const item of recipe.steps) seen(item.ref);
  for (const item of recipe.equipment ?? []) seen(item.ref);
  let next = maxRef + 1;

  const groups = (titles: LocalizedText[] | undefined): GroupForm[] =>
    (titles ?? []).map((title) => ({
      key: next++,
      name: readText(title, lang),
    }));
  const ingredientGroups = groups(recipe.ingredient_groups);
  const stepGroups = groups(recipe.step_groups);
  const groupKey = (list: GroupForm[], index: number | undefined) =>
    index === undefined ? null : (list[index]?.key ?? null);

  const ingredients: IngredientForm[] = recipe.ingredients.map((item) => ({
    ref: item.ref,
    group: groupKey(ingredientGroups, item.group),
    name: readText(item.name, lang),
    amount: input(engine, item.quantity, lang),
    preparation: readText(item.preparation, lang),
    note: readText(item.note, lang),
    optional: item.optional === true,
    fixed: item.fixed === true,
    alt: (item.alt ?? []).map((q) => input(engine, q, lang)),
    substitutes: (item.substitutes ?? []).map((sub) => ({
      name: readText(sub.name, lang),
      amount: input(engine, sub.quantity, lang),
      orig: sub,
    })),
    orig: item,
  }));

  const equipment: EquipmentForm[] = (recipe.equipment ?? []).map((item) => ({
    ref: item.ref,
    name: readText(item.name, lang),
    quantity: input(engine, item.quantity, lang),
    note: readText(item.note, lang),
    orig: item,
  }));

  const steps: StepForm[] = recipe.steps.map((step) => ({
    ref: step.ref,
    group: groupKey(stepGroups, step.group),
    text: readProse(step.text, lang),
    ingredients: (step.ingredients ?? []).map((pick) => ({
      ref: pick.ref,
      amount: input(engine, pick.quantity, lang),
    })),
    equipment: [...(step.equipment ?? [])],
    timers: (step.timers ?? []).map((timer) => ({
      duration: input(engine, timer.duration, lang),
      label: readText(timer.label, lang),
      passive: timer.passive === true,
    })),
    heat: step.heat ?? "",
    tempMin: step.temperature ? rationalText(step.temperature.min) : "",
    tempMax: step.temperature?.max ? rationalText(step.temperature.max) : "",
    tempUnit: temperatureUnit(step.temperature?.unit),
    tempTarget: step.temperature?.target ?? "",
    produces: readText(step.produces, lang),
    note: readProse(step.note, lang),
    image: step.blob ?? null,
    orig: step,
  }));

  const times = { prep: "", cook: "", rest: "", total: "" };
  for (const key of TIME_KEYS) {
    times[key] = input(engine, recipe.times?.[key], lang);
  }

  const blobs: Record<string, CrispBlob> = {};
  for (const [id, blob] of Object.entries(asRecord(payload.blobs))) {
    blobs[id] = blob as CrispBlob;
  }

  return {
    v: FORM_VERSION,
    lang,
    base: payload,
    identity: recipe.identity,
    title: readText(recipe.title, lang),
    summary: readProse(recipe.summary, lang),
    yieldAmount: input(engine, recipe.yield.amount, lang),
    yieldUnit: readText(recipe.yield.unit_text, lang),
    times,
    ingredientGroups,
    ingredients,
    equipment,
    stepGroups,
    steps,
    media: [...(recipe.media ?? [])],
    blobs,
    course: [...(recipe.course ?? [])],
    diet: [...(recipe.diet ?? [])],
    allergens: [...(recipe.allergens ?? [])],
    cuisine: [...(recipe.cuisine ?? [])],
    tags: [...(recipe.tags ?? [])],
    notes: readProse(recipe.notes, lang),
    source: {
      title: recipe.source?.title ?? "",
      author: recipe.source?.author ?? "",
      url: recipe.source?.url ?? "",
    },
    license: payload.meta.license,
    nextRef: next,
  };
}

// ----------------------------------------------------------------- saving

/** Keys of a recipe the editor owns; everything else is carried over. */
const EDITED_RECIPE_KEYS = [
  "title",
  "summary",
  "yield",
  "times",
  "ingredient_groups",
  "ingredients",
  "equipment",
  "step_groups",
  "steps",
  "tags",
  "cuisine",
  "course",
  "diet",
  "allergens",
  "media",
  "notes",
  "source",
] as const;

function omit<T extends object, K extends keyof T>(
  value: T,
  keys: readonly K[],
): Omit<T, K> {
  const copy = { ...value };
  for (const key of keys) delete copy[key];
  return copy;
}

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

/** Converts a form to the wire draft `seal` and `validateDraft` take. */
export function formToDraft(
  form: FormState,
  engine: AmountEngine,
): DraftResult {
  const { lang } = form;
  const problems: FieldProblem[] = [];

  const amount = (
    text: string,
    field: string,
    label: string,
  ): Quantity | undefined => {
    const typed = text.trim();
    if (!typed) return undefined;
    try {
      return engine.parseAmount(typed, lang).quantity;
    } catch (error: unknown) {
      problems.push({
        field,
        label,
        message: `Could not read "${typed}" as an amount (${String(error instanceof Error ? error.message : error)}).`,
      });
      return undefined;
    }
  };

  const duration = (
    text: string,
    field: string,
    label: string,
  ): Quantity | undefined => {
    const typed = text.trim();
    if (!typed) return undefined;
    try {
      return engine.parseDuration(typed);
    } catch {
      problems.push({
        field,
        label,
        message: `"${typed}" is not a duration — try 45 min, 1h 30m or 20-25 min.`,
      });
      return undefined;
    }
  };

  const number = (
    text: string,
    field: string,
    label: string,
  ): Rational | undefined => {
    const typed = text.trim().replace(/°.*$/, "").trim();
    if (!typed) return undefined;
    try {
      const parsed = engine.parseAmount(typed, lang).quantity;
      if (parsed.min && parsed.max === undefined && parsed.unit === undefined) {
        return parsed.min;
      }
    } catch {
      // Reported below.
    }
    problems.push({
      field,
      label,
      message: `"${typed}" is not a number.`,
    });
    return undefined;
  };

  // Groups: a blank, unused section is dropped; positions are re-indexed.
  const ingredients = outputIngredients(form);
  const steps = outputSteps(form);
  const equipment = outputEquipment(form);
  const usedIngredientGroups = new Set(ingredients.map((i) => i.group));
  const usedStepGroups = new Set(steps.map((s) => s.group));
  const ingredientGroups = form.ingredientGroups.filter(
    (g) => g.name.trim() || usedIngredientGroups.has(g.key),
  );
  const stepGroups = form.stepGroups.filter(
    (g) => g.name.trim() || usedStepGroups.has(g.key),
  );
  const groupIndex = (groups: GroupForm[], key: number | null) => {
    if (key === null) return undefined;
    const index = groups.findIndex((g) => g.key === key);
    return index < 0 ? undefined : index;
  };

  const base = form.base;
  const baseRecipe: Recipe | undefined = base?.recipes[0];

  // Ingredients.
  const wireIngredients: Ingredient[] = ingredients.map((item, i) => {
    const where = `Ingredient ${i + 1}`;
    const orig = item.orig;
    const out: Ingredient = {
      ...(orig
        ? omit(orig, [
            "group",
            "name",
            "quantity",
            "alt",
            "fixed",
            "preparation",
            "note",
            "optional",
            "substitutes",
          ])
        : {}),
      ref: item.ref,
      name: requiredText(lang, item.name, orig?.name),
    };
    const group = groupIndex(ingredientGroups, item.group);
    if (group !== undefined) out.group = group;
    const quantity = amount(
      item.amount,
      fieldId.ingredient(item.ref, "amount"),
      `${where} · amount`,
    );
    if (quantity) out.quantity = quantity;
    const alt: Quantity[] = [];
    item.alt.forEach((text, j) => {
      const field = fieldId.ingredientAlt(item.ref, j);
      const q = amount(text, field, `${where} · measure ${j + 1}`);
      if (q) alt.push(q);
      else if (!text.trim()) {
        problems.push({
          field,
          label: `${where} · measure ${j + 1}`,
          message: "Enter the other measure, or remove it.",
        });
      }
    });
    if (alt.length) out.alt = alt;
    if (item.fixed) out.fixed = true;
    const preparation = writeText(lang, item.preparation, orig?.preparation);
    if (preparation) out.preparation = preparation;
    const note = writeText(lang, item.note, orig?.note);
    if (note) out.note = note;
    if (item.optional) out.optional = true;
    if (item.substitutes.length) {
      out.substitutes = item.substitutes.map((sub, j): Substitute => {
        const s: Substitute = {
          ...(sub.orig ? omit(sub.orig, ["name", "quantity"]) : {}),
          name: requiredText(lang, sub.name, sub.orig?.name),
        };
        const q = amount(
          sub.amount,
          fieldId.substitute(item.ref, j, "amount"),
          `${where} · substitute ${j + 1}`,
        );
        if (q) s.quantity = q;
        return s;
      });
    }
    return out;
  });

  // Equipment.
  const wireEquipment: Equipment[] = equipment.map((item, i) => {
    const orig = item.orig;
    const out: Equipment = {
      ...(orig ? omit(orig, ["name", "quantity", "note"]) : {}),
      ref: item.ref,
      name: requiredText(lang, item.name, orig?.name),
    };
    const quantity = amount(
      item.quantity,
      fieldId.equipment(item.ref, "quantity"),
      `Equipment ${i + 1} · quantity`,
    );
    if (quantity) out.quantity = quantity;
    const note = writeText(lang, item.note, orig?.note);
    if (note) out.note = note;
    return out;
  });

  // Steps.
  const wireSteps: Step[] = steps.map((step, i) => {
    const where = `Step ${i + 1}`;
    const orig = step.orig;
    const out: Step = {
      ...(orig
        ? omit(orig, [
            "group",
            "text",
            "ingredients",
            "equipment",
            "timers",
            "heat",
            "temperature",
            "produces",
            "note",
            "blob",
          ])
        : {}),
      ref: step.ref,
      text: requiredProse(lang, step.text, orig?.text),
    };
    const group = groupIndex(stepGroups, step.group);
    if (group !== undefined) out.group = group;
    if (step.ingredients.length) {
      out.ingredients = step.ingredients.map((pick, j): StepIngredient => {
        const q = amount(
          pick.amount,
          fieldId.stepPick(step.ref, j),
          `${where} · ingredient ${j + 1}`,
        );
        return q ? { ref: pick.ref, quantity: q } : { ref: pick.ref };
      });
    }
    if (step.equipment.length) out.equipment = [...step.equipment];
    if (step.timers.length) {
      out.timers = step.timers.map((timer, j): Timer => {
        const field = fieldId.timer(step.ref, j, "duration");
        const label = `${where} · timer ${j + 1}`;
        let q = duration(timer.duration, field, label);
        if (!timer.duration.trim()) {
          problems.push({ field, label, message: "Enter how long." });
        }
        q ??= {};
        const t: Timer = { duration: q };
        const text = writeText(lang, timer.label);
        if (text) t.label = text;
        if (timer.passive) t.passive = true;
        return t;
      });
    }
    if (step.heat) out.heat = step.heat;
    const min = number(
      step.tempMin,
      fieldId.step(step.ref, "temp-min"),
      `${where} · temperature`,
    );
    const max = number(
      step.tempMax,
      fieldId.step(step.ref, "temp-max"),
      `${where} · temperature up to`,
    );
    if (min) {
      const temperature: Temperature = { min, unit: step.tempUnit };
      if (max) temperature.max = max;
      if (step.tempTarget) temperature.target = step.tempTarget;
      out.temperature = temperature;
    } else if (step.tempMax.trim() || step.tempTarget) {
      if (!step.tempMin.trim()) {
        problems.push({
          field: fieldId.step(step.ref, "temp-min"),
          label: `${where} · temperature`,
          message: "Enter the temperature.",
        });
      }
    }
    const produces = writeText(lang, step.produces, orig?.produces);
    if (produces) out.produces = produces;
    const note = writeProse(lang, step.note, orig?.note);
    if (note) out.note = note;
    if (step.image) out.blob = step.image;
    return out;
  });

  // Whole-recipe fields.
  const times: Times = {};
  for (const key of TIME_KEYS) {
    const q = duration(
      form.times[key],
      fieldId.time(key),
      `${TIME_LABELS[key as TimeKey]} time`,
    );
    if (q) times[key] = q;
  }

  let yieldAmount = amount(form.yieldAmount, fieldId.yieldAmount, "Yield");
  if (!form.yieldAmount.trim()) {
    problems.push({
      field: fieldId.yieldAmount,
      label: "Yield",
      message: "Enter how much this makes, e.g. 4 or 24.",
    });
  }
  yieldAmount ??= {};

  const recipe: Recipe = {
    ...(baseRecipe ? omit(baseRecipe, EDITED_RECIPE_KEYS) : {}),
    identity: form.identity,
    id: baseRecipe?.id ?? "",
    revision: baseRecipe?.revision ?? "",
    modified_at: baseRecipe?.modified_at ?? 0,
    title: requiredText(lang, form.title, baseRecipe?.title),
    yield: {
      amount: yieldAmount,
      unit_text: requiredText(
        lang,
        form.yieldUnit,
        baseRecipe?.yield.unit_text,
      ),
    },
    ingredients: wireIngredients,
    steps: wireSteps,
  };
  const summary = writeProse(lang, form.summary, baseRecipe?.summary);
  if (summary) recipe.summary = summary;
  if (Object.keys(times).length) recipe.times = times;
  if (ingredientGroups.length) {
    recipe.ingredient_groups = ingredientGroups.map((g, i) =>
      requiredText(lang, g.name, baseRecipe?.ingredient_groups?.[i]),
    );
  }
  if (wireEquipment.length) recipe.equipment = wireEquipment;
  if (stepGroups.length) {
    recipe.step_groups = stepGroups.map((g, i) =>
      requiredText(lang, g.name, baseRecipe?.step_groups?.[i]),
    );
  }
  if (form.tags.length) recipe.tags = [...form.tags];
  if (form.cuisine.length) recipe.cuisine = [...form.cuisine];
  if (form.course.length) recipe.course = [...form.course];
  if (form.diet.length) recipe.diet = [...form.diet];
  if (form.allergens.length) recipe.allergens = [...form.allergens];
  if (form.media.length) recipe.media = [...form.media];
  const notes = writeProse(lang, form.notes, baseRecipe?.notes);
  if (notes) recipe.notes = notes;
  const source: Source = {};
  if (form.source.title.trim()) source.title = form.source.title.trim();
  if (form.source.author.trim()) source.author = form.source.author.trim();
  if (form.source.url.trim()) source.url = form.source.url.trim();
  if (Object.keys(source).length) recipe.source = source;

  // Only blobs something references travel with the draft.
  const referenced = new Set<string>(form.media);
  for (const step of wireSteps) if (step.blob) referenced.add(step.blob);
  for (const item of wireIngredients) if (item.blob) referenced.add(item.blob);
  for (const item of wireEquipment) if (item.blob) referenced.add(item.blob);
  const cover = base?.meta.cover;
  if (cover && form.blobs[cover]) referenced.add(cover);
  const blobs: Record<string, CrispBlob> = {};
  for (const id of referenced) {
    const blob = form.blobs[id];
    if (blob) blobs[id] = blob;
  }

  const meta: BundlePayload["meta"] = base
    ? {
        ...omit(base.meta, ["cover"]),
        title: requiredText(lang, form.title, base.meta.title),
        license: form.license,
      }
    : {
        title: requiredText(lang, form.title),
        authors: [],
        license: form.license,
        created_at: 0,
        modified_at: 0,
      };
  if (cover && blobs[cover]) meta.cover = cover;

  const draft: BundlePayload = base
    ? {
        ...base,
        meta,
        recipes: [recipe],
        blobs: asWireMap(blobs),
      }
    : {
        v: 0,
        min_v: 0,
        kind: "recipe",
        doc_id: "",
        default_language: lang,
        languages: [lang],
        meta,
        recipes: [recipe],
        blobs: asWireMap(blobs),
      };

  return {
    draft,
    index: {
      ingredients: ingredients.map((i) => i.ref),
      steps: steps.map((s) => s.ref),
      equipment: equipment.map((e) => e.ref),
      ingredientGroups: ingredientGroups.map((g) => g.key),
      stepGroups: stepGroups.map((g) => g.key),
    },
    problems,
  };
}
