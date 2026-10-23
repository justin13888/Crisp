import { describe, expect, test } from "bun:test";
import type { DraftIndex } from "./convert";
import {
  issuesToProblems,
  locatePointer,
  mergeProblems,
  pointerSegments,
  problemsByField,
} from "./issues";
import { fieldId } from "./model";

const index: DraftIndex = {
  ingredients: [7, 3],
  steps: [9, 4],
  equipment: [12],
  ingredientGroups: [20, 21],
  stepGroups: [30],
};

const at = (pointer: string) => locatePointer(pointer, index).field;

describe("pointerSegments", () => {
  test("splits and unescapes RFC 6901 pointers", () => {
    expect(pointerSegments("")).toEqual([]);
    expect(pointerSegments("/a/b~1c/d~0e/0")).toEqual(["a", "b/c", "d~e", "0"]);
  });
});

describe("locatePointer", () => {
  test("title, identity title, and meta title map to the title", () => {
    expect(at("/recipes/0/title/en")).toBe(fieldId.title);
    expect(at("/recipes/0/identity/title")).toBe(fieldId.title);
    expect(at("/meta/title/en")).toBe(fieldId.title);
  });

  test("whole-recipe fields", () => {
    expect(at("/recipes/0/summary/en/0/0/t")).toBe(fieldId.summary);
    expect(at("/recipes/0/yield/amount")).toBe(fieldId.yieldAmount);
    expect(at("/recipes/0/yield/unit_text/en")).toBe(fieldId.yieldUnit);
    expect(at("/recipes/0/times/cook/unit")).toBe(fieldId.time("cook"));
    expect(at("/recipes/0/tags/2")).toBe(fieldId.tags);
    expect(at("/recipes/0/cuisine/0")).toBe(fieldId.cuisine);
    expect(at("/recipes/0/allergens/0")).toBe(fieldId.allergens);
    expect(at("/recipes/0/media/1")).toBe(fieldId.media);
    expect(at("/recipes/0/notes/en")).toBe(fieldId.notes);
    expect(at("/recipes/0/source/url")).toBe(fieldId.source("url"));
    expect(at("/recipes/0/source/author")).toBe(fieldId.source("author"));
    expect(at("/meta/license")).toBe(fieldId.license);
    expect(at("/blobs/blb1xyz/size")).toBe(fieldId.media);
  });

  test("ingredients map through the output index to refs", () => {
    expect(at("/recipes/0/ingredients/1/name/en")).toBe(
      fieldId.ingredient(3, "name"),
    );
    expect(at("/recipes/0/ingredients/0/quantity/unit")).toBe(
      fieldId.ingredient(7, "amount"),
    );
    expect(at("/recipes/0/ingredients/0/preparation/en")).toBe(
      fieldId.ingredient(7, "preparation"),
    );
    expect(at("/recipes/0/ingredients/0/alt/2")).toBe(
      fieldId.ingredientAlt(7, 2),
    );
    expect(at("/recipes/0/ingredients/1/substitutes/0/quantity")).toBe(
      fieldId.substitute(3, 0, "amount"),
    );
    expect(at("/recipes/0/ingredients/1/substitutes/0/name/en")).toBe(
      fieldId.substitute(3, 0, "name"),
    );
    expect(at("/recipes/0/ingredients/1/group")).toBe(
      fieldId.ingredient(3, "name"),
    );
    expect(at("/recipes/0/ingredients")).toBe(fieldId.ingredients);
    expect(at("/recipes/0/ingredients/9/name")).toBe(fieldId.ingredients);
  });

  test("groups map to their section title fields", () => {
    expect(at("/recipes/0/ingredient_groups/1/en")).toBe(
      fieldId.ingredientGroup(21),
    );
    expect(at("/recipes/0/step_groups/0")).toBe(fieldId.stepGroup(30));
  });

  test("equipment", () => {
    expect(at("/recipes/0/equipment/0/name/en")).toBe(
      fieldId.equipment(12, "name"),
    );
    expect(at("/recipes/0/equipment/0/quantity")).toBe(
      fieldId.equipment(12, "quantity"),
    );
  });

  test("steps and their parts", () => {
    expect(at("/recipes/0/steps/1/text/en/0/0/t")).toBe(
      fieldId.step(4, "text"),
    );
    expect(at("/recipes/0/steps/0/ingredients/1/ref")).toBe(
      fieldId.stepPick(9, 1),
    );
    expect(at("/recipes/0/steps/0/equipment/0")).toBe(
      fieldId.step(9, "equipment"),
    );
    expect(at("/recipes/0/steps/0/timers/2/duration/unit")).toBe(
      fieldId.timer(9, 2, "duration"),
    );
    expect(at("/recipes/0/steps/0/timers/2/label/en")).toBe(
      fieldId.timer(9, 2, "label"),
    );
    expect(at("/recipes/0/steps/0/temperature/max")).toBe(
      fieldId.step(9, "temp-max"),
    );
    expect(at("/recipes/0/steps/0/temperature")).toBe(
      fieldId.step(9, "temp-min"),
    );
    expect(at("/recipes/0/steps/1/blob")).toBe(fieldId.step(4, "image"));
    expect(at("/recipes/0/steps/1/produces/en")).toBe(
      fieldId.step(4, "produces"),
    );
    expect(at("/recipes/0/steps")).toBe(fieldId.steps);
  });

  test("unplaceable pointers have no field but a label", () => {
    expect(locatePointer("", index)).toEqual({ field: null, label: "Recipe" });
    expect(at("/languages/0")).toBeNull();
  });

  test("labels name positions for the summary list", () => {
    expect(locatePointer("/recipes/0/steps/1/timers/0", index).label).toBe(
      "Step 2 · timer 1",
    );
    expect(locatePointer("/recipes/0/ingredients/0/name", index).label).toBe(
      "Ingredient 1",
    );
  });
});

describe("problems", () => {
  test("issuesToProblems keeps messages", () => {
    expect(
      issuesToProblems(
        [{ path: "/recipes/0/title/en", code: "E.SCHEMA", message: "empty" }],
        index,
      ),
    ).toEqual([{ field: fieldId.title, label: "Title", message: "empty" }]);
  });

  test("mergeProblems prefers local parse messages per field", () => {
    const local = [{ field: "f-a", label: "A", message: "not a duration" }];
    const engine = [
      { field: "f-a", label: "A", message: "needs min or text" },
      { field: "f-b", label: "B", message: "empty" },
      { field: "f-b", label: "B", message: "empty" },
    ];
    expect(mergeProblems(local, engine)).toEqual([
      { field: "f-a", label: "A", message: "not a duration" },
      { field: "f-b", label: "B", message: "empty" },
    ]);
  });

  test("problemsByField groups messages and skips unplaced ones", () => {
    const map = problemsByField([
      { field: "f-a", label: "A", message: "one" },
      { field: "f-a", label: "A", message: "two" },
      { field: null, label: "R", message: "three" },
    ]);
    expect([...map]).toEqual([["f-a", ["one", "two"]]]);
  });
});
