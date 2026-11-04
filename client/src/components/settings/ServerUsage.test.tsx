import { afterAll, describe, expect, mock, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import type { AccountUsageView } from "../../generated/crisp";
import * as servers from "../../lib/servers";
import type { ServerContextValue } from "../../lib/servers";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

// `ServerUsage` reads the connection from `useServer`; a fake context stands
// in for the provider so each test chooses the status, session, and usage.
let context: Partial<ServerContextValue> = {};
let usageCalls = 0;

mock.module("../../lib/servers", () => ({
  ...servers,
  useServer: () => context,
}));

const React = await import("react");
const { act } = React;
const { createRoot } = await import("react-dom/client");
const { ServerUsage } = await import("./ServerUsage");

const MIB = 1024 * 1024;

function usage(overrides: Partial<AccountUsageView> = {}): AccountUsageView {
  return {
    bytes_used: 10 * MIB,
    bytes_limit: 256 * MIB,
    bundles_used: 12,
    bundles_limit: 5000,
    ...overrides,
  };
}

function signedIn(read: () => Promise<AccountUsageView>) {
  context = {
    status: "online",
    username: "ada",
    usage: () => {
      usageCalls++;
      return read();
    },
  };
}

async function render(): Promise<HTMLElement> {
  const host = document.createElement("div");
  document.body.replaceChildren(host);
  const root = createRoot(host);
  await act(async () => {
    root.render(React.createElement(ServerUsage));
  });
  // Let the usage promise settle and its state update render.
  await act(async () => {});
  return host;
}

describe("ServerUsage", () => {
  afterAll(async () => {
    await GlobalRegistrator.unregister();
  });

  test("renders nothing and reads nothing when signed out", async () => {
    usageCalls = 0;
    context = {
      status: "online",
      username: null,
      usage: async () => {
        usageCalls++;
        return usage();
      },
    };
    const host = await render();
    expect(host.querySelector("[data-testid=server-usage]")).toBeNull();
    expect(usageCalls).toBe(0);
  });

  test("shows storage and revisions against the quotas when signed in", async () => {
    usageCalls = 0;
    signedIn(async () => usage());
    const host = await render();
    expect(usageCalls).toBe(1);
    const text = host.querySelector("[data-testid=server-usage]")?.textContent;
    expect(text).toContain("Published storage");
    expect(text).toContain("12 of 5000");
    expect(host.querySelector("[role=status]")).toBeNull();
  });

  test("warns when the account is nearly full", async () => {
    signedIn(async () => usage({ bytes_used: 250 * MIB }));
    const host = await render();
    expect(host.querySelector("[role=status]")?.textContent).toContain(
      "nearly full",
    );
  });

  test("says why when the usage cannot be read", async () => {
    signedIn(async () => {
      throw new Error("server unreachable");
    });
    const host = await render();
    const text = host.querySelector("[data-testid=server-usage]")?.textContent;
    expect(text).toContain("Couldn’t read your storage on this server");
    expect(text).toContain("server unreachable");
  });
});
