import { describe, expect, test } from "bun:test";
import type { IngredientMatch } from "../../lib/crisp";
import {
  exactLink,
  type LinkEngine,
  relink,
  SUGGESTION_LIMIT,
  suggestions,
} from "./linking";

/** `butter` and `salted butter` link exactly; `but` suggests two entries. */
const linkEngine: LinkEngine & { calls: number[] } = {
  calls: [],
  matchIngredient(text: string, _tag: string, limit: number): IngredientMatch {
    this.calls.push(limit);
    const folded = text.trim().toLowerCase();
    if (folded === "butter") {
      return { kind: "exact", slug: "butter", name: "butter" };
    }
    if (folded === "salted butter") {
      return { kind: "exact", slug: "salted-butter" };
    }
    if (folded === "but") {
      return {
        kind: "candidates",
        candidates: ["butter", "peanut-butter"].map((slug) => ({
          slug,
          name: slug.replace("-", " "),
          spelling: slug,
          shard: "en-US",
          exact_region: false,
        })),
      };
    }
    return { kind: "none" };
  },
};

describe("exactLink", () => {
  test("links only an exact match, and never a blank name", () => {
    expect(exactLink(linkEngine, "Butter", "en")).toBe("butter");
    expect(exactLink(linkEngine, "but", "en")).toBeUndefined();
    expect(exactLink(linkEngine, "   ", "en")).toBeUndefined();
  });
});

describe("relink", () => {
  test("attaches an exact match as the author finishes typing it", () => {
    expect(relink(linkEngine, "en", "butte", "butter", undefined)).toBe(
      "butter",
    );
  });

  test("replaces an automatic link when the name links elsewhere", () => {
    expect(relink(linkEngine, "en", "butter", "salted butter", "butter")).toBe(
      "salted-butter",
    );
  });

  test("drops an automatic link once the name no longer matches it", () => {
    expect(
      relink(linkEngine, "en", "butter", "butter beans", "butter"),
    ).toBeUndefined();
  });

  test("keeps a code the author picked or the bundle carried", () => {
    expect(relink(linkEngine, "en", "but", "but", "peanut-butter")).toBe(
      "peanut-butter",
    );
    expect(relink(linkEngine, "en", "ghee", "ghee, melted", "ghee")).toBe(
      "ghee",
    );
    // A code this index does not know survives every edit.
    expect(relink(linkEngine, "en", "yuzu", "yuzu kosho", "yuzu-kosho")).toBe(
      "yuzu-kosho",
    );
  });

  test("respects a removed chip while the name links the same way", () => {
    expect(relink(linkEngine, "en", "butter", "Butter ", undefined)).toBe(
      undefined,
    );
  });
});

describe("suggestions", () => {
  test("ranks candidates, offers a removed exact link back, else nothing", () => {
    expect(suggestions(linkEngine, "but", "en")).toEqual([
      { slug: "butter", name: "butter" },
      { slug: "peanut-butter", name: "peanut butter" },
    ]);
    expect(suggestions(linkEngine, "salted butter", "en")).toEqual([
      { slug: "salted-butter", name: "salted-butter" },
    ]);
    expect(suggestions(linkEngine, "quinoa", "en")).toEqual([]);
    expect(suggestions(linkEngine, "", "en")).toEqual([]);
  });

  test("asks the engine for a bounded list", () => {
    linkEngine.calls.length = 0;
    suggestions(linkEngine, "but", "en");
    expect(linkEngine.calls).toEqual([SUGGESTION_LIMIT]);
  });
});
