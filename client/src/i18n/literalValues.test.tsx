import { afterAll, describe, expect, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
afterAll(() => GlobalRegistrator.unregister());

const { act } = await import("react");
const { createRoot } = await import("react-dom/client");
const { i18n, LITERAL_VALUES, Trans } = await import("./index");

/** Render the signed-in line for `username` and return its container. */
function renderSignedIn(username: string): HTMLElement {
  const container = document.createElement("div");
  const root = createRoot(container);
  const t = i18n.getFixedT("en", "settings");
  act(() =>
    root.render(
      <Trans
        t={t}
        i18nKey="server.signedInAs"
        {...LITERAL_VALUES}
        values={{ username }}
        components={{ strong: <strong /> }}
      />,
    ),
  );
  return container;
}

describe("LITERAL_VALUES", () => {
  test.each([
    "<strong>x</strong>",
    "<0>zz</0>",
    "a & b",
    "&amp; &lt;",
    "'quoted' \"name\" / slash",
  ])("renders %p literally inside the markup", (username) => {
    const container = renderSignedIn(username);
    expect(container.textContent).toBe(`Signed in as ${username}`);
    const strong = container.querySelectorAll("strong");
    expect(strong.length).toBe(1);
    expect(strong[0]?.textContent).toBe(username);
  });

  test("leaves the translation's own markup intact", () => {
    const container = renderSignedIn("ada");
    expect(container.innerHTML).toBe("Signed in as <strong>ada</strong>");
  });
});
