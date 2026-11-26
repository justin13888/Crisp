import { describe, expect, test } from "bun:test";
import type { IngredientName } from "../../lib/crisp";
import {
  MAX_SUGGESTION_TEXT,
  type SuggestEngine,
  suggestionFailureMessage,
  suggestionFor,
  TOO_MANY_REQUESTS,
} from "./suggest";

/** French files under `fr-FR`; Dutch has no shard; butter is named in French. */
const engine: SuggestEngine = {
  ingredientShard: (tag) => (tag.startsWith("fr") ? "fr-FR" : undefined),
  ingredientName: (slug): IngredientName | undefined => {
    if (slug === "butter") return { name: "beurre", shard: "fr-FR" };
    if (slug === "kimchi") return { name: "kimchi", shard: "en-US" };
    return undefined;
  },
};

describe("suggestionFor", () => {
  test("an unlinked name proposes a new entry in the recipe's shard", () => {
    expect(
      suggestionFor(engine, "fr", "  piment d'Espelette ", undefined),
    ).toEqual({
      kind: "new_entry",
      locale: "fr-FR",
      text: "piment d'Espelette",
    });
  });

  test("an entry with no name in the recipe's shard proposes the typed name", () => {
    expect(suggestionFor(engine, "fr", "kimchi", "kimchi")).toEqual({
      kind: "name",
      locale: "fr-FR",
      text: "kimchi",
      slug: "kimchi",
    });
    // A code this index does not know has no name anywhere.
    expect(suggestionFor(engine, "fr", "yuzu", "yuzu")?.kind).toBe("name");
  });

  test("nothing is proposed when there is nothing to propose", () => {
    expect(suggestionFor(engine, "fr", "beurre", "butter")).toBeUndefined();
    expect(suggestionFor(engine, "fr", "   ", undefined)).toBeUndefined();
    expect(suggestionFor(engine, "nl", "boter", undefined)).toBeUndefined();
    const long = "a".repeat(MAX_SUGGESTION_TEXT + 1);
    expect(suggestionFor(engine, "fr", long, undefined)).toBeUndefined();
    const longest = "a".repeat(MAX_SUGGESTION_TEXT);
    expect(suggestionFor(engine, "fr", longest, undefined)?.text).toBe(longest);
  });
});

/** A failure shaped like the engine's. */
function failure(message: string, code: string, status: number) {
  return Object.assign(new Error(message), {
    name: "CrispFailure",
    code,
    status,
  });
}

describe("suggestionFailureMessage", () => {
  test("an exhausted budget says to wait; the rest is the server's detail", () => {
    const tooMany = suggestionFailureMessage(
      failure("Too Many Requests", TOO_MANY_REQUESTS, 429),
    );
    expect(tooMany).not.toBe("Too Many Requests");
    expect(tooMany.length).toBeGreaterThan(0);
    expect(
      suggestionFailureMessage(
        failure("you already hold 50 open suggestions", "conflict", 409),
      ),
    ).toBe("you already hold 50 open suggestions");
    expect(suggestionFailureMessage(new Error("offline"))).toBe("offline");
  });
});
