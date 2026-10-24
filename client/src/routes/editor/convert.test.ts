import { describe, expect, test } from "bun:test";
import type * as Wasm from "../../generated/crisp";
import type {
  BundlePayload,
  Identity,
  ParsedAmount,
  ParsedTemperature,
  Quantity,
} from "../../generated/crisp";
import {
  type AmountEngine,
  draftToForm,
  formToDraft,
  newForm,
} from "./convert";
import { blankIngredient, blankStep, type FormState, fieldId } from "./model";
import { asRecord } from "./wire";

/** A tiny deterministic stand-in for the engine's amount parsing. */
const UNITS: Record<string, string> = {
  cup: "[cup_us]",
  cups: "[cup_us]",
  g: "g",
  tsp: "[tsp_us]",
  min: "min",
  h: "h",
};

function ratio(text: string): [number, number] | null {
  const frac = /^(\d+)\/(\d+)$/.exec(text);
  if (frac) return [Number(frac[1]), Number(frac[2])];
  return /^\d+$/.test(text) ? [Number(text), 1] : null;
}

const stub: AmountEngine = {
  parseAmount(text: string, lang: string): ParsedAmount {
    if (!text.trim()) throw new Error("the amount is empty");
    const [first = "", second, ...rest] = text.trim().split(/\s+/);
    const min = ratio(first);
    if (!min)
      return {
        quantity: { text: { [lang]: text } as never },
        unit_known: false,
      };
    const range = /^(\d+)-(\d+)$/.exec(first);
    const quantity: Quantity = range
      ? { min: [Number(range[1]), 1], max: [Number(range[2]), 1] }
      : { min };
    const unitText = [second, ...rest].filter(Boolean).join(" ");
    if (!unitText) return { quantity, unit_known: false };
    const unit = UNITS[unitText];
    return unit
      ? { quantity: { ...quantity, unit }, unit_known: true }
      : { quantity: { ...quantity, unit: `{${unitText}}` }, unit_known: false };
  },
  parseDuration(text: string): Quantity {
    const match = /^(\d+)\s*(min|h)?$/.exec(text.trim());
    if (!match) throw new Error(`"${text}" is not a duration`);
    return { min: [Number(match[1]), 1], unit: match[2] ?? "min" };
  },
  parseTemperature(text: string, lang: string): ParsedTemperature {
    const { min, max, unit } = stub.parseAmount(text, lang).quantity;
    if (!min || (unit && unit !== "{C}")) throw new Error("not a temperature");
    return { min, ...(max ? { max } : {}), ...(unit ? { unit: "Cel" } : {}) };
  },
  quantityToInput(quantity: Quantity): string {
    const [n, d] = quantity.min ?? [0, 1];
    const num = d === 1 ? String(n) : `${n}/${d}`;
    const unit = Object.entries(UNITS).find(
      ([, code]) => code === quantity.unit,
    );
    return unit ? `${num} ${unit[0]}` : num;
  },
};

const identity: Identity = {
  author_fpr: new Uint8Array(),
  title: "",
  language: "en",
  created_at: 0,
  birth_nonce: new Uint8Array(16),
};

/** A filled-in form: two groups, two ingredients, two steps. */
function sample(): FormState {
  const form = newForm({ lang: "en", identity });
  return {
    ...form,
    title: "Tomato soup",
    summary: "Warming.\n\nQuick, too.",
    yieldAmount: "4",
    yieldUnit: "bowls",
    times: { prep: "10", cook: "1 h", rest: "", total: "" },
    ingredientGroups: [{ key: 10, name: "Soup" }],
    ingredients: [
      { ...blankIngredient(1, 10), name: "tomatoes", amount: "500 g" },
      {
        ...blankIngredient(3),
        name: "salt",
        optional: true,
        fixed: true,
        alt: ["1 tsp"],
        substitutes: [{ name: "soy sauce", amount: "" }],
      },
    ],
    equipment: [{ ref: 4, name: "pot", quantity: "1", note: "large" }],
    steps: [
      {
        ...blankStep(2),
        text: "Chop.\n\nSimmer.",
        ingredients: [{ ref: 1, amount: "1/2" }],
        equipment: [4],
        timers: [{ duration: "20 min", label: "simmer", passive: true }],
        heat: "medium",
        tempMin: "90 C",
        tempTarget: "water",
      },
    ],
    tags: ["weeknight"],
    course: ["main"],
    source: { title: "", author: "Nonna", url: "https://example.com/soup" },
    nextRef: 11,
  };
}

describe("newForm", () => {
  test("starts with one blank ingredient and step and a fresh counter", () => {
    const form = newForm({ lang: "fr", identity, tempUnit: "[degF]" });
    expect(form.lang).toBe("fr");
    expect(form.license).toBe("CC-BY-4.0");
    expect(form.ingredients.map((i) => i.ref)).toEqual([1]);
    expect(form.steps.map((s) => s.ref)).toEqual([2]);
    expect(form.steps[0]?.tempUnit).toBe("[degF]");
    expect(form.nextRef).toBe(3);
  });
});

describe("formToDraft", () => {
  test("builds a recipe payload with blank derived fields", () => {
    const { draft, problems } = formToDraft(sample(), stub);
    expect(problems).toEqual([]);
    expect(draft.kind).toBe("recipe");
    expect(draft.default_language).toBe("en");
    expect(draft.languages).toEqual(["en"]);
    expect(draft.doc_id).toBe("");
    expect(draft.meta.license).toBe("CC-BY-4.0");
    expect(asRecord(draft.meta.title)).toEqual({ en: "Tomato soup" });
    const recipe = draft.recipes[0];
    expect(recipe?.id).toBe("");
    expect(recipe?.identity).toBe(identity);
    expect(asRecord(recipe?.title)).toEqual({ en: "Tomato soup" });
    expect(asRecord(recipe?.summary)).toEqual({
      en: [[{ t: "Warming." }], [{ t: "Quick, too." }]],
    });
    expect(recipe?.yield.amount).toEqual({ min: [4, 1] });
    expect(recipe?.times).toEqual({
      prep: { min: [10, 1], unit: "min" },
      cook: { min: [1, 1], unit: "h" },
    });
    expect(recipe?.tags).toEqual(["weeknight"]);
    expect(recipe?.course).toEqual(["main"]);
    expect(recipe?.cuisine).toBeUndefined();
    expect(recipe?.source).toEqual({
      author: "Nonna",
      url: "https://example.com/soup",
    });
  });

  test("orders ungrouped ingredients first and indexes groups", () => {
    const { draft, index } = formToDraft(sample(), stub);
    const [salt, tomato] = draft.recipes[0]?.ingredients ?? [];
    expect(index.ingredients).toEqual([3, 1]);
    expect(index.ingredientGroups).toEqual([10]);
    expect(salt).toEqual({
      ref: 3,
      name: { en: "salt" } as never,
      alt: [{ min: [1, 1], unit: "[tsp_us]" }],
      fixed: true,
      optional: true,
      substitutes: [{ name: { en: "soy sauce" } as never }],
    });
    expect(tomato).toEqual({
      ref: 1,
      group: 0,
      name: { en: "tomatoes" } as never,
      quantity: { min: [500, 1], unit: "g" },
    });
  });

  test("omits empty optional fields and never emits undefined", () => {
    const { draft } = formToDraft(sample(), stub);
    const json = JSON.stringify(draft, (_k, v) =>
      v instanceof Uint8Array ? [] : v === undefined ? "UNDEFINED" : v,
    );
    expect(json).not.toContain("UNDEFINED");
    expect(json).not.toContain("null");
  });

  test("converts steps: portions, equipment, timers, heat, temperature", () => {
    const step = formToDraft(sample(), stub).draft.recipes[0]?.steps[0];
    expect(step).toEqual({
      ref: 2,
      text: { en: [[{ t: "Chop." }], [{ t: "Simmer." }]] } as never,
      ingredients: [{ ref: 1, quantity: { min: [1, 2] } }],
      equipment: [4],
      timers: [
        {
          duration: { min: [20, 1], unit: "min" },
          label: { en: "simmer" } as never,
          passive: true,
        },
      ],
      heat: "medium",
      temperature: { min: [90, 1], unit: "Cel", target: "water" },
    });
  });

  test("drops blank rows and unused blank sections", () => {
    const form = sample();
    form.ingredients.push(blankIngredient(20));
    form.steps.push(blankStep(21));
    form.stepGroups.push({ key: 22, name: "" });
    const { draft, index } = formToDraft(form, stub);
    expect(index.ingredients).toEqual([3, 1]);
    expect(index.steps).toEqual([2]);
    expect(draft.recipes[0]?.step_groups).toBeUndefined();
  });

  test("reports unparseable durations, temperatures, and empty yield", () => {
    const form = sample();
    form.yieldAmount = "";
    form.times.rest = "a while";
    const step = form.steps[0];
    if (!step) throw new Error("no step");
    step.timers = [{ duration: "", label: "", passive: false }];
    step.tempMin = "hot";
    step.tempMax = "5 g";
    const { problems } = formToDraft(form, stub);
    const fields = problems.map((p) => p.field);
    expect(fields).toContain(fieldId.yieldAmount);
    expect(fields).toContain(fieldId.time("rest"));
    expect(fields).toContain(fieldId.timer(2, 0, "duration"));
    expect(fields).toContain(fieldId.step(2, "temp-min"));
    expect(fields).toContain(fieldId.step(2, "temp-max"));
  });

  test("only referenced blobs travel with the draft", () => {
    const form = sample();
    const blob = {
      media_type: "image/webp",
      size: 1,
      bytes: new Uint8Array([1]),
    };
    form.blobs = { a: blob, b: blob, c: blob };
    form.media = ["a"];
    if (form.steps[0]) form.steps[0].image = "c";
    const { draft } = formToDraft(form, stub);
    expect(Object.keys(asRecord(draft.blobs)).sort()).toEqual(["a", "c"]);
    expect(draft.recipes[0]?.media).toEqual(["a"]);
    expect(draft.recipes[0]?.steps[0]?.blob).toBe("c");
  });
});

describe("draftToForm", () => {
  test("round-trips a draft and continues refs past the largest", () => {
    const { draft } = formToDraft(sample(), stub);
    const stored: BundlePayload = {
      ...draft,
      doc_id: "rcp1stored",
      recipes: draft.recipes.map((r) => ({
        ...r,
        id: "rcp1stored",
        revision: "rev1old",
      })),
    };
    const form = draftToForm(stored, stub);
    expect(form.base).toBe(stored);
    expect(form.title).toBe("Tomato soup");
    expect(form.summary).toBe("Warming.\n\nQuick, too.");
    expect(form.yieldAmount).toBe("4");
    expect(form.times.cook).toBe("1 h");
    expect(form.ingredients.map((i) => [i.ref, i.name, i.amount])).toEqual([
      [3, "salt", ""],
      [1, "tomatoes", "500 g"],
    ]);
    expect(form.ingredientGroups.map((g) => g.name)).toEqual(["Soup"]);
    expect(form.ingredients[1]?.group).toBe(
      form.ingredientGroups[0]?.key ?? -1,
    );
    expect(form.steps[0]?.tempMin).toBe("90");
    expect(form.steps[0]?.timers[0]?.duration).toBe("20 min");
    expect(form.nextRef).toBeGreaterThan(4);
    for (const group of form.ingredientGroups) {
      expect(group.key).toBeGreaterThan(4);
    }

    // Re-saving keeps the recipe id (a revision) and unedited fields.
    const again = formToDraft(form, stub).draft;
    expect(again.recipes[0]?.id).toBe("rcp1stored");
    expect(again.recipes[0]?.revision).toBe("rev1old");
    expect(again.doc_id).toBe("rcp1stored");
    expect(again.recipes[0]?.ingredients).toEqual(
      draft.recipes[0]?.ingredients ?? [],
    );
    expect(again.recipes[0]?.steps).toEqual(draft.recipes[0]?.steps ?? []);
  });

  test("keeps rich prose and other languages when text is unchanged", () => {
    const { draft } = formToDraft(sample(), stub);
    const recipe = draft.recipes[0];
    if (!recipe) throw new Error("no recipe");
    const rich = {
      en: [[{ t: "Warming.", style: ["bold"] }], [{ t: "Quick, too." }]],
      fr: [[{ t: "Réconfortant." }]],
    };
    const stored: BundlePayload = {
      ...draft,
      languages: ["en", "fr"],
      recipes: [
        {
          ...recipe,
          summary: rich as never,
          title: { en: "Tomato soup", fr: "Soupe" } as never,
          nutrition: { basis: { min: [1, 1] }, values: {} as never },
        },
      ],
    };
    const form = draftToForm(stored, stub);
    const out = formToDraft(form, stub).draft.recipes[0];
    expect(out?.summary).toBe(rich as never);
    expect(asRecord(out?.title)).toEqual({ en: "Tomato soup", fr: "Soupe" });
    expect(out?.nutrition).toEqual(stored.recipes[0]?.nutrition);

    form.summary = "Changed.";
    const changed = formToDraft(form, stub).draft.recipes[0];
    expect(asRecord(changed?.summary)).toEqual({
      en: [[{ t: "Changed." }]],
      fr: [[{ t: "Réconfortant." }]],
    });
  });
});

// ------------------------------------------------- real engine (optional)

const wasmDir = new URL("../../../public/crisp-wasm/", import.meta.url);
const haveWasm = await Bun.file(new URL("crisp_bg.wasm", wasmDir)).exists();

async function realEngine(): Promise<typeof Wasm> {
  const module: typeof Wasm = await import(
    new URL("crisp.js", wasmDir).pathname
  );
  const bytes = await Bun.file(new URL("crisp_bg.wasm", wasmDir)).arrayBuffer();
  module.initSync({ module: bytes });
  return module;
}

describe.skipIf(!haveWasm)("with the real engine", () => {
  test("a complete form validates clean and seals", async () => {
    const engine = await realEngine();
    const form = {
      ...sample(),
      identity: engine.newIdentity("", "en", 0),
    };
    const { draft, problems } = formToDraft(form, engine);
    expect(problems).toEqual([]);
    expect(engine.validateDraft(draft)).toEqual([]);
    const signer = engine.Signer.generate();
    const bytes = signer.seal(draft, "Test Cook", 1_700_000_000, null);
    const bundle = engine.Bundle.open(bytes);
    const reopened = draftToForm(bundle.draft(), engine);
    expect(reopened.title).toBe("Tomato soup");
    expect(reopened.ingredients.map((i) => i.amount)).toEqual(["", "500 g"]);

    // Saving the reopened form with the old bytes makes a revision.
    reopened.title = "Tomato soup II";
    const revised = signer.seal(
      formToDraft(reopened, engine).draft,
      "Test Cook",
      1_700_000_100,
      bytes,
    );
    const next = engine.Bundle.open(revised);
    expect(next.docId).toBe(bundle.docId);
    expect(next.draft().recipes[0]?.supersedes).toBe(
      bundle.draft().recipes[0]?.revision ?? "",
    );
  });

  test("engine issues point at real form fields", async () => {
    const engine = await realEngine();
    const form = sample();
    form.title = "";
    if (form.ingredients[0]) form.ingredients[0].name = "";
    form.source.url = "http://insecure";
    const { draft } = formToDraft(form, engine);
    const paths = engine.validateDraft(draft).map((i) => i.path);
    expect(paths).toContain("/recipes/0/title/en");
    expect(paths).toContain("/recipes/0/source/url");
  });
});
