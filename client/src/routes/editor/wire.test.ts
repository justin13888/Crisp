import { describe, expect, test } from "bun:test";
import { defaultTempUnit, primaryLanguage } from "./model";
import { addSlugs, normalizeSlug, parseSlugs } from "./slugs";
import {
  asRecord,
  readProse,
  readText,
  splitParagraphs,
  textToParagraphs,
  writeProse,
  writeText,
} from "./wire";

describe("localized text", () => {
  test("reads a language, falling back to any language", () => {
    expect(readText({ en: "Soup" }, "en")).toBe("Soup");
    expect(readText({ fr: "Soupe" }, "en")).toBe("Soupe");
    expect(readText(undefined, "en")).toBe("");
  });

  test("writes the edited language and keeps the others", () => {
    expect(asRecord(writeText("en", " Soup ", { fr: "Soupe" }))).toEqual({
      fr: "Soupe",
      en: "Soup",
    });
    expect(writeText("en", "   ")).toBeUndefined();
  });
});

describe("prose", () => {
  test("blank lines separate paragraphs", () => {
    expect(splitParagraphs("a\nb\n\n \nc\r\n\r\nd")).toEqual([
      "a\nb",
      "c",
      "d",
    ]);
    expect(textToParagraphs("x\n\ny")).toEqual([[{ t: "x" }], [{ t: "y" }]]);
  });

  test("reads runs back as text", () => {
    const prose = { en: [[{ t: "Hi " }, { t: "there" }], [{ t: "Bye" }]] };
    expect(readProse(prose, "en")).toBe("Hi there\n\nBye");
  });

  test("keeps the original when the text is unchanged", () => {
    const orig = { en: [[{ t: "Hi", style: ["bold" as const] }]] };
    expect(writeProse("en", "Hi", orig)).toBe(orig);
    expect(asRecord(writeProse("en", "Hello", orig))).toEqual({
      en: [[{ t: "Hello" }]],
    });
    expect(writeProse("en", "\n\n")).toBeUndefined();
  });
});

describe("slugs", () => {
  test("normalizes to lowercase hyphenated ASCII", () => {
    expect(normalizeSlug("  Sichuan Food! ", 64)).toBe("sichuan-food");
    expect(normalizeSlug("Crème brûlée", 64)).toBe("creme-brulee");
    expect(normalizeSlug("--a--b--", 64)).toBe("a-b");
  });

  test("truncates to the given bound without a trailing hyphen", () => {
    expect(normalizeSlug("x".repeat(80), 64)).toHaveLength(64);
    expect(normalizeSlug("ab cd", 3)).toBe("ab");
    expect(
      normalizeSlug("x".repeat(80), Number.POSITIVE_INFINITY),
    ).toHaveLength(80);
  });

  test("splits lists and skips duplicates", () => {
    expect(parseSlugs("Thai, thai; Quick dinner\n", 64)).toEqual([
      "thai",
      "quick-dinner",
    ]);
    expect(addSlugs(["thai"], "Thai, vegan", 64)).toEqual(["thai", "vegan"]);
    expect(addSlugs([], "abcdef", 3)).toEqual(["abc"]);
  });
});

describe("locale helpers", () => {
  test("primary language subtag with a safe fallback", () => {
    expect(primaryLanguage("en-US")).toBe("en");
    expect(primaryLanguage("PT-br")).toBe("pt");
    expect(primaryLanguage("")).toBe("en");
    expect(primaryLanguage(undefined)).toBe("en");
  });

  test("°F only where it is customary", () => {
    expect(defaultTempUnit("en-US")).toBe("[degF]");
    expect(defaultTempUnit("en-GB")).toBe("Cel");
    expect(defaultTempUnit("de")).toBe("Cel");
  });
});
