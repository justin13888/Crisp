import { afterAll, describe, expect, mock, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import * as identity from "../../lib/identity";
import type { IdentityStatus } from "../../lib/identity";
import * as servers from "../../lib/servers";
import type { ServerContextValue } from "../../lib/servers";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

// Fake server and identity contexts stand in for the providers, so each
// test chooses whether the identity is unlocked and how deletion goes.
let context: Partial<ServerContextValue> = {};
let status: IdentityStatus = "unlocked";
let calls: string[] = [];

mock.module("../../lib/servers", () => ({
  ...servers,
  useServer: () => context,
}));
mock.module("../../lib/identity", () => ({
  ...identity,
  useIdentity: () => ({ status }),
}));
mock.module("./PassphraseFields", () => ({
  UnlockForm: ({ prompt }: { prompt: string }) => prompt,
}));

const React = await import("react");
const { act } = React;
const { createRoot } = await import("react-dom/client");
const { AccountDeletion } = await import("./AccountDeletion");

function signedIn(
  deletion: () => Promise<void> = async () => {},
  identityStatus: IdentityStatus = "unlocked",
) {
  calls = [];
  status = identityStatus;
  context = {
    url: "https://crisp.example",
    username: "ada",
    deleteAccount: () => {
      calls.push("deleteAccount");
      return deletion();
    },
  };
}

async function render(): Promise<HTMLElement> {
  const host = document.createElement("div");
  document.body.replaceChildren(host);
  const root = createRoot(host);
  await act(async () => {
    root.render(React.createElement(AccountDeletion, { username: "ada" }));
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

describe("AccountDeletion", () => {
  afterAll(async () => {
    await GlobalRegistrator.unregister();
  });

  test("asks before deleting, and says what stays", async () => {
    signedIn();
    const host = await render();
    await click(button(host, "Delete account"));
    expect(calls).toEqual([]);
    const dialog = host.querySelector("[role=alertdialog]")?.textContent;
    expect(dialog).toContain("Delete ada from https://crisp.example");
    expect(dialog).toContain("Your library on this device");
    await click(button(host, "Cancel"));
    button(host, "Delete account");
  });

  test("deletes after confirming with an unlocked identity", async () => {
    signedIn();
    const host = await render();
    await click(button(host, "Delete account"));
    await click(button(host, "Sign in again and delete"));
    expect(calls).toEqual(["deleteAccount"]);
    expect(host.querySelector("[role=alert]")).toBeNull();
  });

  test("a locked identity is asked to unlock instead", async () => {
    signedIn(async () => {}, "locked");
    const host = await render();
    await click(button(host, "Delete account"));
    expect(host.textContent).toContain("Unlock your identity");
    expect(() => button(host, "Sign in again and delete")).toThrow();
  });

  test("says why when the server refuses, and allows a retry", async () => {
    signedIn(async () => {
      const error = new Error("too old");
      error.name = "CrispFailure";
      throw Object.assign(error, {
        code: "reauthentication-required",
        status: 403,
      });
    });
    const host = await render();
    await click(button(host, "Delete account"));
    await click(button(host, "Sign in again and delete"));
    const alert = host.querySelector("[role=alert]")?.textContent;
    expect(alert).toContain("Couldn’t delete the account");
    expect(alert).toContain("newer sign-in");
    expect(button(host, "Sign in again and delete").disabled).toBe(false);
  });
});
