import { describe, expect, test } from "bun:test";
import {
  formatQuantity,
  formatRational,
  resolveProse,
  resolveText,
} from "./render";

describe("formatRational", () => {
  test("renders whole numbers plainly", () => {
    expect(formatRational([4, 1])).toBe("4");
  });
  test("reduces to lowest terms", () => {
    expect(formatRational([10, 4])).toBe("2½");
  });
  test("uses vulgar fractions", () => {
    expect(formatRational([1, 2])).toBe("½");
    expect(formatRational([3, 4])).toBe("¾");
  });
  test("renders mixed numbers", () => {
    expect(formatRational([3, 2])).toBe("1½");
  });
});

describe("resolveText", () => {
  test("prefers a preferred language", () => {
    expect(resolveText({ en: "Hi", fr: "Salut" }, ["fr"], "en")).toBe("Salut");
  });
  test("falls back to the default language", () => {
    expect(resolveText({ en: "Hi" }, ["de"], "en")).toBe("Hi");
  });
  test("falls back to any value", () => {
    expect(resolveText({ it: "Ciao" }, ["de"], "en")).toBe("Ciao");
  });
  test("is empty for an absent map", () => {
    expect(resolveText(undefined, [], "en")).toBe("");
  });
});

describe("resolveProse", () => {
  test("resolves to the default language", () => {
    const prose = resolveProse({ en: [[{ t: "Mix." }]] }, [], "en");
    expect(prose).toEqual([[{ t: "Mix." }]]);
  });
});

describe("formatQuantity", () => {
  test("formats a number and unit", () => {
    expect(formatQuantity({ min: [2, 1], unit: "cup" }, [], "en")).toBe(
      "2 cup",
    );
  });
  test("formats a range", () => {
    expect(
      formatQuantity({ min: [1, 1], max: [2, 1], unit: "g" }, [], "en"),
    ).toBe("1–2 g");
  });
  test("marks an approximate amount", () => {
    expect(
      formatQuantity({ min: [1, 1], unit: "tsp", approx: true }, [], "en"),
    ).toBe("~1 tsp");
  });
  test("falls back to a text amount", () => {
    expect(formatQuantity({ text: { en: "to taste" } }, [], "en")).toBe(
      "to taste",
    );
  });
});
