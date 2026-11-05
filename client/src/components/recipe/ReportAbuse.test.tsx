import { afterAll, afterEach, describe, expect, mock, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import type { ServerInfoView } from "../../generated/crisp";
import * as servers from "../../lib/servers";
import type { ServerContextValue } from "../../lib/servers";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

// Both components read `info` from `useServer`; a fake context stands in
// for the provider so each test chooses what the server advertises.
let context: Partial<ServerContextValue> = {};

mock.module("../../lib/servers", () => ({
  ...servers,
  useServer: () => context,
}));

const React = await import("react");
const { act } = React;
const { createRoot } = await import("react-dom/client");
const { ReportAbuse } = await import("./ReportAbuse");
const { ServerTerms } = await import("../settings/ServerTerms");

/** A server that advertises `abuse_contact` and `terms_url`. */
function advertising(
  abuse_contact: string | undefined,
  terms_url: string | undefined,
) {
  const info: ServerInfoView = {
    api_versions: ["v1"],
    core_version: "0.1.0",
    format_version_text: "1.1.0",
    min_format_version_text: "1.0.0",
    platform_keys: [],
    platform_user_id: "usr1server",
    abuse_contact,
    terms_url,
  };
  context = { status: "online", info };
}

// Roots mounted by the current test, unmounted after it so none stays
// subscribed to the shared i18n instance and re-renders against another
// file's mocked context when a later test changes the locale.
const mounted: { unmount: () => void }[] = [];

afterEach(() => {
  for (const root of mounted.splice(0)) act(() => root.unmount());
});

async function render(element: React.ReactElement): Promise<HTMLElement> {
  const host = document.createElement("div");
  document.body.replaceChildren(host);
  const root = createRoot(host);
  mounted.push(root);
  await act(async () => {
    root.render(element);
  });
  return host;
}

afterAll(async () => {
  await GlobalRegistrator.unregister();
});

describe("ReportAbuse", () => {
  test("links the abuse contact, naming the recipe", async () => {
    advertising("abuse@crisp.example", undefined);
    const host = await render(
      React.createElement(ReportAbuse, { docId: "rcp1" }),
    );
    const link = host.querySelector("a");
    expect(link?.getAttribute("href")).toBe(
      "mailto:abuse@crisp.example?subject=Report%3A%20rcp1",
    );
    expect(link?.textContent).toBe("Report it to the server operator");
  });

  test("shows a contact no link can carry as text", async () => {
    advertising("call the front desk", undefined);
    const host = await render(
      React.createElement(ReportAbuse, { docId: "rcp1" }),
    );
    expect(host.querySelector("a")).toBeNull();
    expect(host.textContent).toContain("call the front desk");
  });

  test("offers nothing without a contact or a server", async () => {
    advertising(undefined, "https://crisp.example/terms");
    const element = React.createElement(ReportAbuse, { docId: "rcp1" });
    expect((await render(element)).textContent).toBe("");
    context = { status: "offline", info: null };
    expect((await render(element)).textContent).toBe("");
  });
});

describe("ServerTerms", () => {
  test("links the terms a first sign-in agrees to", async () => {
    advertising(undefined, "https://crisp.example/terms");
    const host = await render(React.createElement(ServerTerms));
    const link = host.querySelector("a");
    expect(link?.getAttribute("href")).toBe("https://crisp.example/terms");
    expect(link?.getAttribute("rel")).toBe("noopener noreferrer");
    expect(link?.textContent).toBe("terms of service");
    expect(host.textContent).toBe(
      "Signing in for the first time creates an account on this server, under its terms of service.",
    );
  });

  test("offers nothing without terms or with terms no link may carry", async () => {
    advertising("abuse@crisp.example", undefined);
    expect((await render(React.createElement(ServerTerms))).textContent).toBe(
      "",
    );
    advertising(undefined, "javascript:alert(1)");
    expect((await render(React.createElement(ServerTerms))).textContent).toBe(
      "",
    );
  });
});
