import { afterAll, describe, expect, mock, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import * as library from "../lib/library";
import * as servers from "../lib/servers";
import type { ServerContextValue } from "../lib/servers";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

// A fake connection and library record each call, so each test chooses
// how the server answers.
let context: Partial<ServerContextValue> = {};
let calls: string[] = [];

mock.module("../lib/servers", () => ({
  ...servers,
  useServer: () => context,
}));
mock.module("../lib/library", () => ({
  ...library,
  markUnpublished: async (docId: string, url: string, username: string) => {
    calls.push(`markUnpublished:${docId}:${url}:${username}`);
  },
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

async function render(
  props: { revision?: string; publishedTo?: unknown } = {},
): Promise<HTMLElement> {
  const host = document.createElement("div");
  document.body.replaceChildren(host);
  const root = createRoot(host);
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
    expect(calls).toEqual([
      "unpublish:rcp1:all",
      "markUnpublished:rcp1:https://crisp.example:ada",
    ]);
    expect(host.textContent).toContain("no longer offers your revisions");
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
    expect(button(host, "All my revisions").disabled).toBe(false);
  });
});
