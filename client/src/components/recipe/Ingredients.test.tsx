import { afterAll, afterEach, describe, expect, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import type { IngredientSection, IngredientView } from "../../lib/crisp";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
afterAll(() => GlobalRegistrator.unregister());

const { act } = await import("react");
const { createRoot } = await import("react-dom/client");
const { setDerivedMeasures } = await import("../settings/derivedMeasures");
const { IngredientSections, readerIndexName } = await import("./Ingredients");

function item(fields: Partial<IngredientView>): IngredientView {
  return {
    ref: 1,
    name: "flour",
    amount: "1 cup",
    alt: [],
    optional: false,
    fixed: false,
    substitutes: [],
    ...fields,
  };
}

/** The text of the rendered list, then unmount it. */
function render(items: IngredientView[]): string {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  const sections: IngredientSection[] = [{ items }];
  act(() => root.render(<IngredientSections sections={sections} />));
  const text = container.textContent ?? "";
  act(() => root.unmount());
  container.remove();
  return text;
}

afterEach(() => setDerivedMeasures(false));

describe("density-derived measures", () => {
  test("stay hidden until the reader turns them on", () => {
    expect(render([item({ derived: "≈ 140 g" })])).not.toContain("≈");
    setDerivedMeasures(true);
    expect(render([item({ derived: "≈ 140 g" })])).toContain("(≈ 140 g)");
  });

  test("never appear beside an author's own measure", () => {
    setDerivedMeasures(true);
    const text = render([item({ alt: ["125 g"], derived: "≈ 140 g" })]);
    expect(text).toContain("(125 g)");
    expect(text).not.toContain("≈");
  });
});

describe("the index name in the reader's locale", () => {
  /** French names for `butter` only; `kimchi` falls back to English. */
  const engine = {
    ingredientName: (slug: string, tag: string) =>
      slug === "butter" && tag.startsWith("fr")
        ? { name: "beurre", shard: "fr-FR" }
        : slug === "butter" || slug === "kimchi"
          ? { name: slug, shard: "en-US" }
          : undefined,
  };

  test("is shown for a coded ingredient in another language", () => {
    const butter = { name: "Butter", code: "butter" };
    expect(readerIndexName(engine, butter, "de-DE", "fr")).toBe("beurre");
  });

  test("is omitted when it would add nothing", () => {
    const butter = { name: "Butter", code: "butter" };
    // The recipe is already in the reader's language.
    expect(readerIndexName(engine, butter, "fr-CA", "fr")).toBeUndefined();
    // Only an English fallback exists for the reader's French.
    expect(
      readerIndexName(engine, { name: "김치", code: "kimchi" }, "ko", "fr"),
    ).toBeUndefined();
    // The index name is the recipe's own.
    expect(
      readerIndexName(engine, { name: "BUTTER", code: "butter" }, "de", "en"),
    ).toBeUndefined();
    // No code, an unknown code, no language, or no engine yet.
    expect(readerIndexName(engine, { name: "x" }, "de", "fr")).toBeUndefined();
    expect(
      readerIndexName(engine, { name: "x", code: "nope" }, "de", "fr"),
    ).toBeUndefined();
    expect(readerIndexName(engine, butter, undefined, "fr")).toBeUndefined();
    expect(readerIndexName(null, butter, "de", "fr")).toBeUndefined();
  });

  test("renders nothing extra before the engine has loaded", () => {
    const text = render([item({ name: "Mehl", code: "flour" })]);
    expect(text).toBe("1 cup Mehl");
  });
});
