import { afterAll, afterEach, describe, expect, mock, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import * as servers from "../../lib/servers";
import type { ServerContextValue } from "../../lib/servers";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

// `SessionActions` reads the connection from `useServer`; a fake context
// stands in for the provider so each test chooses how signing out goes.
let context: Partial<ServerContextValue> = {};
let calls: string[] = [];

mock.module("../../lib/servers", () => ({
  ...servers,
  useServer: () => context,
}));

const React = await import("react");
const { act } = React;
const { createRoot } = await import("react-dom/client");
const { SessionActions } = await import("./SessionActions");

function signedIn(everywhere: () => Promise<void>) {
  calls = [];
  context = {
    status: "online",
    username: "ada",
    signOut: async () => {
      calls.push("signOut");
    },
    signOutEverywhere: () => {
      calls.push("signOutEverywhere");
      return everywhere();
    },
  };
}

// Roots mounted by the current test, unmounted after it so none stays
// subscribed to the shared i18n instance and re-renders against another
// file's mocked context when a later test changes the locale.
const mounted: { unmount: () => void }[] = [];

afterEach(() => {
  for (const root of mounted.splice(0)) act(() => root.unmount());
});

async function render(): Promise<HTMLElement> {
  const host = document.createElement("div");
  document.body.replaceChildren(host);
  const root = createRoot(host);
  mounted.push(root);
  await act(async () => {
    root.render(React.createElement(SessionActions, { username: "ada" }));
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
  // Let the sign-out promise settle and its state updates render.
  await act(async () => {});
}

describe("SessionActions", () => {
  afterAll(async () => {
    await GlobalRegistrator.unregister();
  });

  test("names the signed-in account", async () => {
    signedIn(async () => {});
    const host = await render();
    expect(host.textContent).toContain("Signed in as ada");
  });

  test("signs out on this device only", async () => {
    signedIn(async () => {});
    const host = await render();
    await click(button(host, "Sign out"));
    expect(calls).toEqual(["signOut"]);
  });

  test("signs out everywhere without an error when the server agrees", async () => {
    signedIn(async () => {});
    const host = await render();
    await click(button(host, "Sign out everywhere"));
    expect(calls).toEqual(["signOutEverywhere"]);
    expect(host.querySelector("[role=alert]")).toBeNull();
    expect(button(host, "Sign out everywhere").disabled).toBe(false);
  });

  test("says why and allows a retry when signing out everywhere fails", async () => {
    signedIn(async () => {
      throw new Error("server unreachable");
    });
    const host = await render();
    await click(button(host, "Sign out everywhere"));
    const alert = host.querySelector("[role=alert]")?.textContent;
    expect(alert).toContain("Couldn’t sign out everywhere");
    expect(alert).toContain("server unreachable");
    expect(alert).toContain("still signed in here");
    expect(button(host, "Sign out everywhere").disabled).toBe(false);
  });

  test("disables the control while the request is in flight", async () => {
    let finish: () => void = () => {};
    signedIn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const host = await render();
    await click(button(host, "Sign out everywhere"));
    expect(button(host, "Sign out everywhere").disabled).toBe(true);
    await act(async () => finish());
    expect(button(host, "Sign out everywhere").disabled).toBe(false);
  });
});
