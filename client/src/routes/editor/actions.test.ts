import { describe, expect, test } from "bun:test";
import {
  addBulkIngredients,
  addBulkSteps,
  addEquipment,
  addGroup,
  addIngredient,
  addStep,
  allocRefs,
  duplicateIngredient,
  duplicateStep,
  moveGroup,
  moveIngredient,
  moveMedia,
  moveStep,
  moveWithin,
  regroup,
  removeEquipment,
  removeGroup,
  removeIngredient,
  removeStep,
  toggleStepEquipment,
  toggleStepIngredient,
  updateIngredient,
} from "./actions";
import { newForm } from "./convert";
import type { FormState } from "./model";

function fresh(): FormState {
  return newForm({
    lang: "en",
    identity: {
      author_fpr: new Uint8Array(),
      title: "",
      language: "en",
      created_at: 0,
      birth_nonce: new Uint8Array(16),
    },
  });
}

const refs = (form: FormState) => form.ingredients.map((i) => i.ref);

describe("ref allocation", () => {
  test("refs are monotonic and never reused after deletes", () => {
    let form = fresh();
    const a = addIngredient(form);
    form = a.form;
    form = removeIngredient(form, a.ref);
    const b = addIngredient(form);
    expect(b.ref).toBeGreaterThan(a.ref);
    const s = addStep(b.form);
    expect(s.ref).toBeGreaterThan(b.ref);
    const { refs: many, form: after } = allocRefs(s.form, 3);
    expect(many).toEqual([s.ref + 1, s.ref + 2, s.ref + 3]);
    expect(after.nextRef).toBe(s.ref + 4);
  });
});

describe("moveWithin", () => {
  test("swaps with the nearest same-lane neighbor", () => {
    expect(moveWithin(["a", "b", "c"], 0, 1)).toEqual(["b", "a", "c"]);
    expect(
      moveWithin(["a", "B", "c"], 0, 1, (x) => x === x.toLowerCase()),
    ).toEqual(["c", "B", "a"]);
  });

  test("is a no-op at the edges", () => {
    expect(moveWithin(["a", "b"], 0, -1)).toEqual(["a", "b"]);
    expect(moveWithin(["a", "b"], 1, 1)).toEqual(["a", "b"]);
  });
});

describe("ingredients", () => {
  test("add inserts after an anchor in its group, else at the group's end", () => {
    let form = fresh();
    const g = addGroup(form, "ingredient", "Sauce");
    form = g.form;
    const x = addIngredient(form, { group: g.ref });
    form = x.form;
    const y = addIngredient(form, { after: 1 });
    form = y.form;
    expect(refs(form)).toEqual([1, y.ref, x.ref]);
    expect(form.ingredients[1]?.group).toBeNull();
    const z = addIngredient(form, { after: x.ref });
    expect(z.form.ingredients.at(-1)?.group).toBe(g.ref);
  });

  test("duplicate copies fields under a new ref, right after", () => {
    let form = updateIngredient(fresh(), 1, { name: "egg", alt: ["50 g"] });
    const copy = duplicateIngredient(form, 1);
    form = copy.form;
    expect(refs(form)).toEqual([1, copy.ref]);
    expect(form.ingredients[1]?.name).toBe("egg");
    expect(form.ingredients[1]?.alt).toEqual(["50 g"]);
    expect(form.ingredients[1]?.alt).not.toBe(form.ingredients[0]?.alt ?? []);
  });

  test("delete removes the ingredient from steps' picks", () => {
    let form = fresh();
    form = toggleStepIngredient(form, 2, 1);
    expect(form.steps[0]?.ingredients).toEqual([{ ref: 1, amount: "" }]);
    form = removeIngredient(form, 1);
    expect(form.ingredients).toEqual([]);
    expect(form.steps[0]?.ingredients).toEqual([]);
  });

  test("move swaps only within the same group", () => {
    let form = fresh();
    const g = addGroup(form, "ingredient", "B");
    form = g.form;
    const grouped = addIngredient(form, { group: g.ref });
    form = grouped.form;
    const second = addIngredient(form, { group: null });
    form = second.form;
    expect(refs(form)).toEqual([1, second.ref, grouped.ref]);
    form = moveIngredient(form, 1, 1);
    expect(refs(form)).toEqual([second.ref, 1, grouped.ref]);
    // The last ungrouped item cannot move into the group.
    expect(refs(moveIngredient(form, 1, 1))).toEqual(refs(form));
  });

  test("regroup moves an item to the end of another section", () => {
    let form = fresh();
    const g = addGroup(form, "ingredient", "Sauce");
    form = addIngredient(g.form).form;
    form = regroup(form, "ingredient", 1, g.ref);
    expect(form.ingredients.at(-1)).toMatchObject({ ref: 1, group: g.ref });
  });
});

describe("bulk", () => {
  test("pasted sections become groups and replace a lone blank row", () => {
    const form = addBulkIngredients(fresh(), [
      { title: null, items: [{ amount: "2", name: "eggs", preparation: "" }] },
      {
        title: "Sauce",
        items: [{ amount: "1 cup", name: "cream", preparation: "" }],
      },
    ]);
    expect(form.ingredients.map((i) => i.name)).toEqual(["eggs", "cream"]);
    expect(form.ingredientGroups.map((g) => g.name)).toEqual(["Sauce"]);
    expect(form.ingredients[1]?.group).toBe(
      form.ingredientGroups[0]?.key ?? -1,
    );
    expect(new Set(refs(form)).size).toBe(2);
  });

  test("a section matching an existing group joins it", () => {
    const g = addGroup(fresh(), "ingredient", "sauce");
    const form = addBulkIngredients(g.form, [
      {
        title: "Sauce",
        items: [{ amount: "", name: "salt", preparation: "" }],
      },
    ]);
    expect(form.ingredientGroups).toHaveLength(1);
    expect(form.ingredients.at(-1)?.group).toBe(g.ref);
  });

  test("pasted steps replace a lone blank step", () => {
    const form = addBulkSteps(fresh(), ["Mix.", "Bake."]);
    expect(form.steps.map((s) => s.text)).toEqual(["Mix.", "Bake."]);
    expect(form.steps.every((s) => s.ref >= 3)).toBe(true);
  });
});

describe("steps", () => {
  test("add, duplicate, move, delete", () => {
    let form = fresh();
    const b = addStep(form, { after: 2 });
    form = b.form;
    const dup = duplicateStep(form, 2);
    form = dup.form;
    expect(form.steps.map((s) => s.ref)).toEqual([2, dup.ref, b.ref]);
    form = moveStep(form, b.ref, -1);
    expect(form.steps.map((s) => s.ref)).toEqual([2, b.ref, dup.ref]);
    form = removeStep(form, 2);
    expect(form.steps.map((s) => s.ref)).toEqual([b.ref, dup.ref]);
  });

  test("new steps inherit the temperature unit last used", () => {
    let form = fresh();
    form = {
      ...form,
      steps: form.steps.map((s) => ({
        ...s,
        tempMin: "350",
        tempUnit: "[degF]",
      })),
    };
    expect(addStep(form).form.steps.at(-1)?.tempUnit).toBe("[degF]");
  });
});

describe("equipment", () => {
  test("delete removes it from steps", () => {
    const e = addEquipment(fresh());
    let form = toggleStepEquipment(e.form, 2, e.ref);
    expect(form.steps[0]?.equipment).toEqual([e.ref]);
    form = removeEquipment(form, e.ref);
    expect(form.equipment).toEqual([]);
    expect(form.steps[0]?.equipment).toEqual([]);
  });
});

describe("groups", () => {
  test("removing a group ungroups its members", () => {
    let form = fresh();
    const g = addGroup(form, "step", "Prep");
    form = regroup(g.form, "step", 2, g.ref);
    form = removeGroup(form, "step", g.ref);
    expect(form.stepGroups).toEqual([]);
    expect(form.steps[0]?.group).toBeNull();
  });

  test("groups reorder", () => {
    let form = fresh();
    const a = addGroup(form, "ingredient", "A");
    const b = addGroup(a.form, "ingredient", "B");
    form = moveGroup(b.form, "ingredient", b.ref, -1);
    expect(form.ingredientGroups.map((g) => g.name)).toEqual(["B", "A"]);
  });

  test("group keys come from the shared ref counter", () => {
    const g = addGroup(fresh(), "ingredient");
    expect(g.ref).toBe(3);
    expect(addIngredient(g.form).ref).toBe(4);
  });
});

describe("media", () => {
  test("photos reorder (the first is the cover)", () => {
    const form = { ...fresh(), media: ["a", "b"] };
    expect(moveMedia(form, "b", -1).media).toEqual(["b", "a"]);
  });
});
