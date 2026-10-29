import { afterAll, afterEach, describe, expect, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
afterAll(() => GlobalRegistrator.unregister());

const { act } = await import("react");
const { createRoot } = await import("react-dom/client");
const { currentLocale, setLocale, storedLocale } = await import("../../i18n");
const { LanguageCard } = await import("./LanguageCard");

/** Mount the card and return its `<select>` and an unmount function. */
function mount(): { select: HTMLSelectElement; unmount: () => void } {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  act(() => root.render(<LanguageCard />));
  const select = container.querySelector("select");
  if (!select) throw new Error("LanguageCard rendered no <select>");
  return {
    select,
    unmount: () => {
      act(() => root.unmount());
      container.remove();
    },
  };
}

/** Pick `value` in the picker the way a user would. */
function choose(select: HTMLSelectElement, value: string): void {
  act(() => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

afterEach(() => setLocale(null));

describe("LanguageCard", () => {
  test("offers the system option and every supported locale", () => {
    const { select, unmount } = mount();
    expect([...select.options].map((o) => o.value)).toEqual([
      "system",
      "en",
      "fr",
    ]);
    unmount();
  });

  test("starts on the system option when no choice is stored", () => {
    const { select, unmount } = mount();
    expect(select.value).toBe("system");
    unmount();
  });

  test("starts on the stored choice", () => {
    setLocale("fr");
    const { select, unmount } = mount();
    expect(select.value).toBe("fr");
    unmount();
  });

  test("picking a locale switches and stores it", () => {
    const { select, unmount } = mount();
    choose(select, "fr");
    expect(storedLocale()).toBe("fr");
    expect(currentLocale()).toBe("fr");
    expect(select.value).toBe("fr");
    unmount();
  });

  test("picking the system option forgets the stored choice", () => {
    setLocale("fr");
    const { select, unmount } = mount();
    choose(select, "system");
    expect(storedLocale()).toBeNull();
    expect(currentLocale()).toBe("en");
    expect(select.value).toBe("system");
    unmount();
  });
});
