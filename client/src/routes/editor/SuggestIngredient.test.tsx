import { afterAll, afterEach, describe, expect, mock, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import type {
  CrispServer,
  Engine,
  FiledSuggestion,
  IngredientSuggestion,
} from "../../lib/crisp";
import * as servers from "../../lib/servers";
import type { ServerContextValue } from "../../lib/servers";
import type { EditorContextValue } from "./context";
import type { LinkedName } from "./IngredientLink";
import type { FormState } from "./model";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
afterAll(() => GlobalRegistrator.unregister());

// The action reads the connection from `useServer`; a fake context stands in
// for the provider so each test chooses the session and the server's answer.
let context: Partial<ServerContextValue> = {};
let sent: IngredientSuggestion[] = [];

mock.module("../../lib/servers", () => ({
  ...servers,
  useServer: () => context,
}));

const { act } = await import("react");
const { createRoot } = await import("react-dom/client");
const { EditorProvider } = await import("./context");
const { LinkedNameField } = await import("./IngredientLink");

/** French files under `fr-FR`; nothing links but `beurre`. */
const engine = {
  matchIngredient: (text: string) =>
    text === "beurre"
      ? { kind: "exact", slug: "butter", name: "beurre" }
      : { kind: "none" },
  ingredientName: (slug: string) =>
    slug === "butter" ? { name: "beurre", shard: "fr-FR" } : undefined,
  ingredientShard: (tag: string) => (tag === "fr" ? "fr-FR" : undefined),
} as unknown as Engine;

const editor = {
  engine,
  form: { lang: "fr" } as unknown as FormState,
  problems: new Map(),
  touch: () => {},
} as unknown as EditorContextValue;

/** Signs in to a server that answers each suggestion with `answer`. */
function signedIn(
  answer: (s: IngredientSuggestion) => Promise<FiledSuggestion>,
) {
  const client = {
    suggestIngredient: (s: IngredientSuggestion) => {
      sent.push(s);
      return answer(s);
    },
  } as unknown as CrispServer;
  context = { username: "ada", client: async () => client };
}

const mounted: (() => void)[] = [];
afterEach(() => {
  for (const unmount of mounted.splice(0)) unmount();
  context = {};
  sent = [];
});

/** Renders a suggestable name field, focused. */
function mount(value: LinkedName) {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  act(() =>
    root.render(
      <EditorProvider value={editor}>
        <LinkedNameField
          id="f-name"
          label="Ingredient 1"
          placeholder="flour"
          value={value}
          onChange={() => {}}
          suggestable
        />
      </EditorProvider>,
    ),
  );
  mounted.push(() => {
    act(() => root.unmount());
    container.remove();
  });
  const input = container.querySelector("input");
  act(() => input?.focus());
  return {
    container,
    button: () =>
      container.querySelector<HTMLButtonElement>(".ingredient-suggest button"),
    text: () => container.querySelector(".ingredient-suggest")?.textContent,
  };
}

/** Clicks `button` and lets the submission settle. */
async function press(button: HTMLButtonElement | null) {
  await act(async () => {
    button?.click();
  });
}

describe("SuggestIngredient", () => {
  test("an unlinked name offers the action only while focused", () => {
    const field = mount({ name: "piment", code: undefined });
    expect(field.button()?.textContent).toBe("Suggest a missing ingredient");
    act(() => field.container.querySelector("input")?.blur());
    expect(field.button()).toBeNull();
  });

  test("a name the recipe's shard already has offers nothing", () => {
    expect(mount({ name: "beurre", code: "butter" }).button()).toBeNull();
    expect(mount({ name: "  ", code: undefined }).button()).toBeNull();
  });

  test("signed out, it explains that suggestions need an account", async () => {
    const field = mount({ name: "piment", code: undefined });
    await press(field.button());
    expect(field.text()).toContain("server account");
    expect(sent).toEqual([]);
  });

  test("signed in, it files the typed name and thanks the author", async () => {
    signedIn(async (s) => ({ id: 1, status: "open", ...s }));
    const field = mount({ name: "piment", code: undefined });
    await press(field.button());
    expect(sent).toEqual([
      { kind: "new_entry", locale: "fr-FR", text: "piment" },
    ]);
    expect(field.button()).toBeNull();
    expect(field.text()).toContain("Suggested");
  });

  test("a linked entry without a French name suggests the typed one", async () => {
    signedIn(async (s) => ({ id: 2, status: "open", ...s }));
    const field = mount({ name: "kimchi", code: "kimchi" });
    expect(field.button()?.textContent).toBe("Suggest “kimchi” as its name");
    await press(field.button());
    expect(sent).toEqual([
      { kind: "name", locale: "fr-FR", text: "kimchi", slug: "kimchi" },
    ]);
  });

  test("a refusal shows the server's problem and leaves the action", async () => {
    signedIn(async () => {
      throw Object.assign(new Error("you already hold 50 open suggestions"), {
        name: "CrispFailure",
        code: "conflict",
        status: 409,
      });
    });
    const field = mount({ name: "piment", code: undefined });
    await press(field.button());
    const alert = field.container.querySelector('[role="alert"]');
    expect(alert?.textContent).toBe("you already hold 50 open suggestions");
    expect(field.button()?.disabled).toBe(false);
  });
});
