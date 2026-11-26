import { afterAll, describe, expect, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import type { Engine, IngredientMatch } from "../../lib/crisp";
import type { EditorContextValue } from "./context";
import type { LinkedName } from "./IngredientLink";
import type { FormState } from "./model";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
afterAll(() => GlobalRegistrator.unregister());

const { act } = await import("react");
const { createRoot } = await import("react-dom/client");
const { EditorProvider } = await import("./context");
const { LinkedNameField } = await import("./IngredientLink");

/** `butter` links exactly; `but` suggests butter and peanut butter. */
const engine = {
  matchIngredient(text: string): IngredientMatch {
    if (text === "butter") {
      return { kind: "exact", slug: "butter", name: "butter" };
    }
    if (text !== "but") return { kind: "none" };
    return {
      kind: "candidates",
      candidates: ["butter", "peanut-butter"].map((slug) => ({
        slug,
        name: slug.replace("-", " "),
        spelling: slug,
        shard: "en-US",
        exact_region: true,
      })),
    };
  },
  ingredientName: (slug: string) =>
    slug === "butter" ? { name: "beurre", shard: "fr-FR" } : undefined,
} as unknown as Engine;

const context = {
  engine,
  form: { lang: "fr" } as unknown as FormState,
  problems: new Map(),
  touch: () => {},
} as unknown as EditorContextValue;

/** Renders the field; returns its parts and every change it reported. */
function mount(value: LinkedName) {
  const changes: LinkedName[] = [];
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  act(() =>
    root.render(
      <EditorProvider value={context}>
        <LinkedNameField
          id="f-name"
          label="Ingredient 1"
          placeholder="flour"
          value={value}
          onChange={(change) => changes.push(change)}
        />
      </EditorProvider>,
    ),
  );
  const input = container.querySelector("input");
  if (!input) throw new Error("no input");
  return {
    container,
    input,
    changes,
    chip: () => container.querySelector(".ingredient-link"),
    offered: () =>
      [...container.querySelectorAll(".ingredient-suggestions button")].map(
        (b) => b.getAttribute("data-code"),
      ),
    unmount: () => {
      act(() => root.unmount());
      container.remove();
    },
  };
}

/** Types `text` into a controlled input the way React observes it. */
function type(input: HTMLInputElement, text: string) {
  const setter = Object.getOwnPropertyDescriptor(
    HTMLInputElement.prototype,
    "value",
  )?.set;
  act(() => {
    setter?.call(input, text);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("LinkedNameField", () => {
  test("an exact name attaches its slug, keeping the typed name", () => {
    const field = mount({ name: "butte", code: undefined });
    type(field.input, "butter");
    expect(field.changes).toEqual([{ name: "butter", code: "butter" }]);
    field.unmount();
  });

  test("a linked name shows a removable chip in the recipe's language", () => {
    const field = mount({ name: "butter", code: "butter" });
    expect(field.chip()?.textContent).toContain("beurre");
    expect(field.chip()?.getAttribute("data-code")).toBe("butter");
    const remove = field.chip()?.querySelector("button");
    act(() => remove?.click());
    expect(field.changes).toEqual([{ name: "butter", code: undefined }]);
    field.unmount();
  });

  test("an unknown code still shows, as its slug", () => {
    const field = mount({ name: "yuzu kosho", code: "yuzu-kosho" });
    expect(field.chip()?.textContent).toContain("yuzu-kosho");
    field.unmount();
  });

  test("suggestions appear while the field has focus and a pick links", () => {
    const field = mount({ name: "but", code: undefined });
    expect(field.offered()).toEqual([]);
    act(() => field.input.focus());
    expect(field.offered()).toEqual(["butter", "peanut-butter"]);
    const pick = field.container.querySelector<HTMLButtonElement>(
      '.ingredient-suggestions [data-code="peanut-butter"]',
    );
    act(() => pick?.click());
    expect(field.changes).toEqual([{ name: "but", code: "peanut-butter" }]);
    field.unmount();
  });

  test("pressing a suggestion or unlink keeps focus on the name", () => {
    // Safari focuses no clicked button: unless the press keeps focus on the
    // input, its blur closes the list before the click lands.
    const press = (button: Element | null | undefined) => {
      const down = new MouseEvent("mousedown", {
        bubbles: true,
        cancelable: true,
      });
      act(() => {
        button?.dispatchEvent(down);
      });
      return down.defaultPrevented;
    };
    const unlinked = mount({ name: "but", code: undefined });
    act(() => unlinked.input.focus());
    const pick = unlinked.container.querySelector(
      '.ingredient-suggestions [data-code="butter"]',
    );
    expect(press(pick)).toBe(true);
    expect(unlinked.offered()).toEqual(["butter", "peanut-butter"]);
    unlinked.unmount();

    const linked = mount({ name: "butter", code: "butter" });
    expect(press(linked.chip()?.querySelector("button"))).toBe(true);
    linked.unmount();
  });

  test("focus leaving the field group closes the suggestions", () => {
    const field = mount({ name: "but", code: undefined });
    act(() => field.input.focus());
    expect(field.offered()).toEqual(["butter", "peanut-butter"]);
    act(() => field.input.blur());
    expect(field.offered()).toEqual([]);
    field.unmount();
  });
});
