import { afterAll, describe, expect, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import type { Engine } from "../../lib/crisp";
import type { EditorContextValue } from "./context";
import type { FormState, IngredientForm } from "./model";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
afterAll(() => GlobalRegistrator.unregister());

const { act } = await import("react");
const { createRoot } = await import("react-dom/client");
const { EditorProvider } = await import("./context");
const { OtherMeasures } = await import("./IngredientSublists");

/** An engine whose only density is flour's: 1 cup → `~140 g`. */
const engine = {
  densityAlternative: (amount: string, code: string) =>
    code === "flour" && amount === "1 cup" ? "~140 g" : undefined,
} as unknown as Engine;

const context = {
  engine,
  form: { lang: "en" } as unknown as FormState,
  problems: new Map(),
  touch: () => {},
} as unknown as EditorContextValue;

function ingredient(code: string | undefined): IngredientForm {
  return {
    ref: 1,
    group: null,
    name: "flour",
    amount: "1 cup",
    preparation: "",
    note: "",
    optional: false,
    fixed: false,
    alt: [],
    substitutes: [],
    ...(code
      ? { orig: { ref: 1, code } as unknown as IngredientForm["orig"] }
      : {}),
  };
}

/** Renders the sublist; returns its fill button (if any) and the patches. */
function mount(item: IngredientForm) {
  const patches: Partial<IngredientForm>[] = [];
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  act(() =>
    root.render(
      <EditorProvider value={context}>
        <OtherMeasures item={item} patch={(change) => patches.push(change)} />
      </EditorProvider>,
    ),
  );
  const fill = [...container.querySelectorAll("button")].find((b) =>
    b.textContent?.includes("density"),
  );
  return {
    fill,
    patches,
    unmount: () => {
      act(() => root.unmount());
      container.remove();
    },
  };
}

describe("Fill from density", () => {
  test("adds the derived measure as an ordinary alt", () => {
    const { fill, patches, unmount } = mount(ingredient("flour"));
    expect(fill?.textContent).toContain("~140 g");
    act(() => fill?.click());
    expect(patches).toEqual([{ alt: ["~140 g"] }]);
    unmount();
  });

  test("is not offered without an index code or a density", () => {
    for (const item of [ingredient(undefined), ingredient("abalone")]) {
      const { fill, unmount } = mount(item);
      expect(fill).toBeUndefined();
      unmount();
    }
  });
});
