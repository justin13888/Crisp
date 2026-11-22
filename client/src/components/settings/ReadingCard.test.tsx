import { afterAll, afterEach, describe, expect, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
afterAll(() => GlobalRegistrator.unregister());

const { act } = await import("react");
const { createRoot } = await import("react-dom/client");
const { DERIVED_MEASURES_KEY, derivedMeasuresEnabled, setDerivedMeasures } =
  await import("./derivedMeasures");
const { ReadingCard } = await import("./ReadingCard");

/** Mount the card and return its checkbox and an unmount function. */
function mount(): { box: HTMLInputElement; unmount: () => void } {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  act(() => root.render(<ReadingCard />));
  const box = container.querySelector("input[type=checkbox]");
  if (!(box instanceof HTMLInputElement)) {
    throw new Error("ReadingCard rendered no checkbox");
  }
  return {
    box,
    unmount: () => {
      act(() => root.unmount());
      container.remove();
    },
  };
}

afterEach(() => localStorage.removeItem(DERIVED_MEASURES_KEY));

describe("derived measures setting", () => {
  test("is off by default", () => {
    expect(derivedMeasuresEnabled()).toBe(false);
    const { box, unmount } = mount();
    expect(box.checked).toBe(false);
    unmount();
  });

  test("ignores a stored value that is not its own", () => {
    localStorage.setItem(DERIVED_MEASURES_KEY, "true");
    expect(derivedMeasuresEnabled()).toBe(false);
  });

  test("the checkbox turns it on and off, and remembers it", () => {
    const { box, unmount } = mount();
    act(() => box.click());
    expect(box.checked).toBe(true);
    expect(derivedMeasuresEnabled()).toBe(true);
    act(() => box.click());
    expect(box.checked).toBe(false);
    expect(localStorage.getItem(DERIVED_MEASURES_KEY)).toBeNull();
    unmount();
  });

  test("a change made elsewhere re-renders the card", () => {
    const { box, unmount } = mount();
    act(() => setDerivedMeasures(true));
    expect(box.checked).toBe(true);
    unmount();
  });
});
