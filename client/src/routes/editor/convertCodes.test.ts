import { describe, expect, test } from "bun:test";
import type { Identity, Quantity } from "../../generated/crisp";
import {
  type AmountEngine,
  draftToForm,
  formToDraft,
  newForm,
} from "./convert";
import { blankIngredient, type FormState } from "./model";

/** No row here has an amount, so the engine is never asked to parse one. */
const engine: AmountEngine = {
  parseAmount() {
    throw new Error("no amounts in this test");
  },
  parseDuration() {
    throw new Error("no durations in this test");
  },
  parseTemperature() {
    throw new Error("no temperatures in this test");
  },
  quantityToInput: (_quantity: Quantity) => "",
};

const identity: Identity = {
  author_fpr: new Uint8Array(),
  title: "",
  language: "en",
  created_at: 0,
  birth_nonce: new Uint8Array(16),
};

/** Butter linked to the index, and salt with a linked substitute. */
function form(): FormState {
  return {
    ...newForm({ lang: "en", identity }),
    title: "Toast",
    ingredients: [
      { ...blankIngredient(1), name: "butter", code: "butter" },
      {
        ...blankIngredient(2),
        name: "salt",
        substitutes: [{ name: "soy sauce", amount: "", code: "soy-sauce" }],
      },
    ],
  };
}

describe("ingredient-index codes", () => {
  test("travel to the wire on ingredients and substitutes", () => {
    const wire = formToDraft(form(), engine).draft.recipes[0]?.ingredients;
    expect(wire?.map((i) => i.code)).toEqual(["butter", undefined]);
    expect(wire?.[1]?.substitutes?.[0]?.code).toBe("soy-sauce");
    expect(Object.hasOwn(wire?.[1] ?? {}, "code")).toBe(false);
  });

  test("load back from a stored draft", () => {
    const { draft } = formToDraft(form(), engine);
    const loaded = draftToForm(draft, engine);
    expect(loaded.ingredients.map((i) => i.code)).toEqual([
      "butter",
      undefined,
    ]);
    expect(loaded.ingredients[1]?.substitutes[0]?.code).toBe("soy-sauce");
  });

  test("removing a loaded code removes it, not restoring the original", () => {
    const loaded = draftToForm(formToDraft(form(), engine).draft, engine);
    const unlinked: FormState = {
      ...loaded,
      ingredients: loaded.ingredients.map((i) => ({
        ...i,
        code: undefined,
        substitutes: i.substitutes.map((s) => ({ ...s, code: undefined })),
      })),
    };
    const wire = formToDraft(unlinked, engine).draft.recipes[0]?.ingredients;
    expect(wire?.some((i) => Object.hasOwn(i, "code"))).toBe(false);
    expect(Object.hasOwn(wire?.[1]?.substitutes?.[0] ?? {}, "code")).toBe(
      false,
    );
  });
});
