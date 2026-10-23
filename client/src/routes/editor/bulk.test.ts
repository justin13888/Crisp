import { describe, expect, test } from "bun:test";
import type { ParsedAmount } from "../../generated/crisp";
import {
  amountPredicate,
  type IsAmount,
  parseIngredientLine,
  parseIngredientList,
  parseStepList,
  stripBullet,
} from "./bulk";

/** Numbers, simple fractions, and a few known units, like the engine. */
const KNOWN = new Set(["cup", "cups", "g", "tsp", "tbsp"]);
const isAmount: IsAmount = (text) => {
  const tokens = text.split(" ");
  let i = 0;
  if (tokens[i] === "about") i++;
  if (!/^\d+([/.-]\d+)?(g)?$/.test(tokens[i] ?? "")) return false;
  i++;
  if (/^\d+\/\d+$/.test(tokens[i] ?? "")) i++;
  if (i === tokens.length) return true;
  return i === tokens.length - 1 && KNOWN.has(tokens[i] ?? "");
};

describe("stripBullet", () => {
  test("removes bullets and numbering but not amounts", () => {
    expect(stripBullet("- 2 eggs")).toBe("2 eggs");
    expect(stripBullet("• salt")).toBe("salt");
    expect(stripBullet("3. Stir")).toBe("Stir");
    expect(stripBullet("1.5 cups milk")).toBe("1.5 cups milk");
    expect(stripBullet("2 eggs")).toBe("2 eggs");
  });
});

describe("parseIngredientLine", () => {
  test("takes the longest leading amount", () => {
    expect(parseIngredientLine("1 1/2 cups flour", isAmount)).toEqual({
      amount: "1 1/2 cups",
      name: "flour",
      preparation: "",
    });
    expect(parseIngredientLine("about 2 cups stock", isAmount).amount).toBe(
      "about 2 cups",
    );
  });

  test("a count with an unknown word keeps the word in the name", () => {
    expect(parseIngredientLine("3 large eggs", isAmount)).toEqual({
      amount: "3",
      name: "large eggs",
      preparation: "",
    });
  });

  test("splits preparation after the first comma and drops 'of'", () => {
    expect(
      parseIngredientLine("200g of butter, softened, cubed", isAmount),
    ).toEqual({
      amount: "200g",
      name: "butter",
      preparation: "softened, cubed",
    });
  });

  test("lines without an amount are all name", () => {
    expect(parseIngredientLine("salt, to taste", isAmount)).toEqual({
      amount: "",
      name: "salt",
      preparation: "to taste",
    });
  });
});

describe("parseIngredientList", () => {
  test("splits lines, skips blanks, and starts sections at colons", () => {
    const text = [
      "2 cups flour",
      "",
      "- 1 tsp salt",
      "For the sauce:",
      "3 tbsp butter",
      "pepper",
      "  ",
    ].join("\r\n");
    expect(parseIngredientList(text, isAmount)).toEqual([
      {
        title: null,
        items: [
          { amount: "2 cups", name: "flour", preparation: "" },
          { amount: "1 tsp", name: "salt", preparation: "" },
        ],
      },
      {
        title: "For the sauce",
        items: [
          { amount: "3 tbsp", name: "butter", preparation: "" },
          { amount: "", name: "pepper", preparation: "" },
        ],
      },
    ]);
  });

  test("a list that opens with a section has no untitled section", () => {
    expect(parseIngredientList("Dough:\n1 cup water", isAmount)).toEqual([
      {
        title: "Dough",
        items: [{ amount: "1 cup", name: "water", preparation: "" }],
      },
    ]);
  });
});

describe("amountPredicate", () => {
  const fake = (result: ParsedAmount | Error) => (_t: string, _l: string) => {
    if (result instanceof Error) throw result;
    return result;
  };

  test("accepts numbers with known or no units", () => {
    expect(
      amountPredicate(
        fake({ quantity: { min: [2, 1] }, unit_known: false }),
        "en",
      )("2"),
    ).toBe(true);
    expect(
      amountPredicate(
        fake({ quantity: { min: [2, 1], unit: "g" }, unit_known: true }),
        "en",
      )("2 g"),
    ).toBe(true);
  });

  test("rejects unknown units, text amounts, and parse failures", () => {
    expect(
      amountPredicate(
        fake({ quantity: { min: [2, 1], unit: "{eggs}" }, unit_known: false }),
        "en",
      )("2 eggs"),
    ).toBe(false);
    expect(
      amountPredicate(fake({ quantity: {}, unit_known: false }), "en")("salt"),
    ).toBe(false);
    expect(amountPredicate(fake(new Error("empty")), "en")("")).toBe(false);
  });
});

describe("parseStepList", () => {
  test("one step per line when there are no blank lines", () => {
    expect(
      parseStepList("1. Heat oil.\n2) Add onions.\nStep 3: Stir."),
    ).toEqual(["Heat oil.", "Add onions.", "Stir."]);
  });

  test("blank lines separate steps and wrapped lines are re-joined", () => {
    expect(
      parseStepList("Heat the oil\nin a pan.\n\n  \nAdd onions.\n"),
    ).toEqual(["Heat the oil in a pan.", "Add onions."]);
  });

  test("empty input has no steps", () => {
    expect(parseStepList("  \n ")).toEqual([]);
  });
});
