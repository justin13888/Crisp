import { afterAll, describe, expect, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
afterAll(() => GlobalRegistrator.unregister());

const { act } = await import("react");
const { createRoot } = await import("react-dom/client");
const { Link, MemoryRouter } = await import("react-router-dom");
const { i18n, SUPPORTED_LOCALES, Trans } = await import("./index");
const { RESOURCES } = await import("./instance");

/** The link text each locale's `save.noIdentity` wraps. */
const LINK_TEXT = {
  en: "Create one in Settings",
  fr: "Créez-en une dans les Réglages",
} as const;

/** HTML void elements: a `<tag>` of one of these names cannot hold text. */
const VOID_ELEMENTS = [
  "area",
  "base",
  "br",
  "col",
  "embed",
  "hr",
  "img",
  "input",
  "link",
  "meta",
  "source",
  "track",
  "wbr",
];

/**
 * Render the save panel's no-identity notice in `locale`, with the same
 * `components` map `routes/editor/SavePanel.tsx` passes.
 */
function renderNotice(locale: (typeof SUPPORTED_LOCALES)[number]): HTMLElement {
  const container = document.createElement("div");
  const root = createRoot(container);
  const t = i18n.getFixedT(locale, "editor");
  act(() =>
    root.render(
      <MemoryRouter>
        <Trans
          t={t}
          i18nKey="save.noIdentity"
          components={{ identityLink: <Link to="/settings" /> }}
        />
      </MemoryRouter>,
    ),
  );
  return container;
}

describe("save.noIdentity", () => {
  test.each([...SUPPORTED_LOCALES])(
    "renders the %p sign-in link text",
    (locale) => {
      const links = renderNotice(locale).querySelectorAll("a");
      expect(links.length).toBe(1);
      expect(links[0]?.getAttribute("href")).toBe("/settings");
      expect(links[0]?.textContent).toBe(LINK_TEXT[locale]);
    },
  );
});

describe("catalogue markup", () => {
  test.each([...SUPPORTED_LOCALES])(
    "%p names no Trans tag after a void HTML element, which drops its text",
    (locale) => {
      const offending: string[] = [];
      for (const table of Object.values(RESOURCES[locale])) {
        for (const [key, text] of Object.entries(table)) {
          if (typeof text !== "string") continue;
          for (const match of text.matchAll(/<([a-zA-Z][\w-]*)>/g)) {
            const tag = match[1] ?? "";
            if (VOID_ELEMENTS.includes(tag.toLowerCase())) {
              offending.push(`${key}: <${tag}>`);
            }
          }
        }
      }
      expect(offending).toEqual([]);
    },
  );
});
