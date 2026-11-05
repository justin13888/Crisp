import {
  afterAll,
  afterEach,
  beforeEach,
  describe,
  expect,
  mock,
  test,
} from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import { publications } from "../lib/library";
import * as servers from "../lib/servers";
import type { ServerContextValue } from "../lib/servers";
import { getStore } from "../lib/store";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

// A fake connection records each call, so each test chooses how the server
// answers. The library is the real one over the preloaded fake IndexedDB:
// `mock.module` is process-wide, so faking it here would replace it for
// every test file that runs after this one.
let context: Partial<ServerContextValue> = {};
let calls: string[] = [];

mock.module("../lib/servers", () => ({
  ...servers,
  useServer: () => context,
}));

const React = await import("react");
const { act } = React;
const { createRoot } = await import("react-dom/client");
const { UnpublishActions } = await import("./UnpublishActions");

function signedIn(
  answer: () => Promise<void> = async () => {},
  username: string | null = "ada",
) {
  calls = [];
  context = {
    url: "https://crisp.example",
    username,
    unpublish: (docId, revision) => {
      calls.push(`unpublish:${docId}:${revision ?? "all"}`);
      return answer();
    },
  };
}

const HERE = [{ url: "https://crisp.example", username: "ada" }];

/** The stored record's publications, as the library reads them back. */
async function storedPublications() {
  const record = await (await getStore()).bundles.get("rcp1");
  return {
    published: record?.published,
    to: publications(record?.publishedTo),
  };
}

beforeEach(async () => {
  await (await getStore()).bundles.put({
    docId: "rcp1",
    kind: "recipe",
    title: "Recipe",
    revision: "rev1",
    bytes: new Uint8Array([1]),
    report: null,
    published: true,
    publishedTo: HERE,
    importedAt: 0,
    updatedAt: 0,
  });
});

// Roots mounted by the current test, unmounted after it so none stays
// subscribed to the shared i18n instance and re-renders against another
// file's mocked context when a later test changes the locale.
const mounted: { unmount: () => void }[] = [];

afterEach(() => {
  for (const root of mounted.splice(0)) act(() => root.unmount());
});

async function render(
  props: { revision?: string; publishedTo?: unknown } = {},
): Promise<HTMLElement> {
  const host = document.createElement("div");
  document.body.replaceChildren(host);
  const root = createRoot(host);
  mounted.push(root);
  await act(async () => {
    root.render(
      React.createElement(UnpublishActions, {
        docId: "rcp1",
        revision: props.revision ?? "rev1",
        publishedTo: "publishedTo" in props ? props.publishedTo : HERE,
      }),
    );
  });
  return host;
}

function button(host: HTMLElement, label: string): HTMLButtonElement {
  const found = [...host.querySelectorAll("button")].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!found) throw new Error(`no button labelled ${label}`);
  return found;
}

async function click(target: HTMLButtonElement) {
  await act(async () => {
    target.click();
  });
  await act(async () => {});
}

/**
 * Flushes React and the fake IndexedDB a bounded number of times until
 * `host` shows `text`, then asserts that it does.
 */
async function settled(host: HTMLElement, text: string) {
  for (let i = 0; i < 20 && !host.textContent?.includes(text); i++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
  expect(host.textContent).toContain(text);
}

describe("UnpublishActions", () => {
  afterAll(async () => {
    await GlobalRegistrator.unregister();
  });

  test("offers nothing when signed out or for an unpublished document", async () => {
    signedIn(async () => {}, null);
    expect((await render()).textContent).toBe("");
    signedIn();
    expect((await render({ publishedTo: undefined })).textContent).toBe("");
  });

  test("offers nothing for a document this account did not publish to this server", async () => {
    signedIn();
    const elsewhere = [{ url: "https://other.example", username: "ada" }];
    expect((await render({ publishedTo: elsewhere })).textContent).toBe("");
    const otherAccount = [{ url: "https://crisp.example", username: "bob" }];
    expect((await render({ publishedTo: otherAccount })).textContent).toBe("");
  });

  test("removes this revision and keeps the local copy", async () => {
    signedIn();
    const host = await render();
    await click(button(host, "Remove from server"));
    await click(button(host, "This revision"));
    expect(calls).toEqual(["unpublish:rcp1:rev1"]);
    expect(await storedPublications()).toEqual({ published: true, to: HERE });
    expect(host.textContent).toContain(
      "This revision is no longer on the server",
    );
    expect(host.textContent).toContain("Your copy on this device stays");
  });

  test("removes every revision and marks the document unpublished", async () => {
    signedIn();
    const host = await render();
    await click(button(host, "Remove from server"));
    await click(button(host, "All my revisions"));
    expect(calls).toEqual(["unpublish:rcp1:all"]);
    // The IndexedDB write settles after `click`'s flush; wait for the
    // render that follows it before reading the store back.
    await settled(host, "no longer offers your revisions");
    expect(await storedPublications()).toEqual({ published: false, to: [] });
  });

  test("offers only the document when the revision is unknown", async () => {
    signedIn();
    const host = await render({ revision: "" });
    await click(button(host, "Remove from server"));
    expect(() => button(host, "This revision")).toThrow();
    button(host, "All my revisions");
  });

  test("says why when the server refuses, and keeps the choice open", async () => {
    signedIn(async () => {
      const error = new Error("published by someone else");
      error.name = "CrispFailure";
      throw Object.assign(error, { code: "forbidden", status: 403 });
    });
    const host = await render();
    await click(button(host, "Remove from server"));
    await click(button(host, "All my revisions"));
    const alert = host.querySelector("[role=alert]")?.textContent;
    expect(alert).toContain("another account published");
    expect(calls).toEqual(["unpublish:rcp1:all"]);
    expect(await storedPublications()).toEqual({ published: true, to: HERE });
    expect(button(host, "All my revisions").disabled).toBe(false);
  });
});
