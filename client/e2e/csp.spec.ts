import { expect, type Page, test } from "@playwright/test";
import {
  field,
  generateIdentity,
  putRecord,
  saveAndUnlock,
  waitForDatabase,
} from "./helpers";

// The Content-Security-Policy (issue #92) against the production build: the
// <meta> policy and `rsbuild preview`'s header are in force, they refuse what
// they should, and the app still loads its engine, unlocks a key, and
// reaches a Crisp server under them. Needs the crisp-core WASM.

const PASSPHRASE = "csp passphrase";
const POLICY = 'meta[http-equiv="Content-Security-Policy"]';

/** A violation the page reported. */
interface Violation {
  directive: string;
  blockedURI: string;
}

/** Records every `securitypolicyviolation` from the first script on. */
async function recordViolations(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const scope = window as unknown as { crispViolations: Violation[] };
    scope.crispViolations = [];
    document.addEventListener("securitypolicyviolation", (event) => {
      scope.crispViolations.push({
        directive: event.effectiveDirective,
        blockedURI: event.blockedURI,
      });
    });
  });
}

/** The violations reported so far on the current document. */
function violations(page: Page): Promise<Violation[]> {
  return page.evaluate(
    () =>
      (window as unknown as { crispViolations?: Violation[] })
        .crispViolations ?? [],
  );
}

/** Answers every request to `origin` with a 503; returns the URLs it saw. */
async function fakeServer(page: Page, origin: string): Promise<string[]> {
  const seen: string[] = [];
  await page.route(`${origin}/**`, (route) => {
    seen.push(route.request().url());
    return route.fulfill({
      status: 503,
      headers: { "access-control-allow-origin": "*" },
      body: "",
    });
  });
  return seen;
}

/** Points Settings' server card at another server. */
async function connectTo(page: Page, url: string): Promise<void> {
  const card = page.getByRole("region", { name: "Server" });
  await card.getByLabel("Server address").fill(url);
  await card.getByRole("button", { name: "Connect" }).click();
}

test.beforeEach(async ({ page }) => {
  await recordViolations(page);
});

test.describe("content security policy", () => {
  test("is sent as a meta tag and, with frame-ancestors, as a header", async ({
    page,
  }) => {
    const response = await page.goto("/");
    const meta = await page.locator(POLICY).getAttribute("content");
    expect(meta).toContain("script-src 'self' 'wasm-unsafe-eval'");
    expect(meta).toContain("object-src 'none'");
    expect(meta).toContain("base-uri 'none'");
    // Browsers ignore frame-ancestors in a <meta> policy.
    expect(meta).not.toContain("frame-ancestors");
    expect(response?.headers()["content-security-policy"]).toBe(
      `${meta}; frame-ancestors 'none'`,
    );
  });

  test("refuses injected script", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("header.topbar .brand")).toHaveText("Crisp");
    // (Playwright's own evaluation is exempt from the policy, so `eval`
    // cannot be probed from here; a script element the page adds is not.)
    const ran = await page.evaluate(() => {
      const scope = window as unknown as { crispInline?: boolean };
      const script = document.createElement("script");
      script.textContent = "window.crispInline = true;";
      document.head.append(script);
      return scope.crispInline === true;
    });
    expect(ran).toBe(false);
    await expect
      .poll(async () => (await violations(page)).map((v) => v.directive))
      .toContain("script-src-elem");
  });

  test("cannot be framed by another site", async ({ page, baseURL }) => {
    await page.route("http://framer.test/", (route) =>
      route.fulfill({
        contentType: "text/html",
        body: `<iframe src="${baseURL}/settings"></iframe>`,
      }),
    );
    await page.goto("http://framer.test/");
    // Chromium replaces a frame its policy refuses with an error page.
    await expect.poll(() => page.frames()[1]?.url()).toMatch(/^chrome-error:/);
    await expect(
      page.frameLocator("iframe").locator("header.topbar .brand"),
    ).toHaveCount(0);
  });

  test("loads the engine, unlocks a key, and reaches servers", async ({
    page,
  }) => {
    test.setTimeout(90_000);
    // The engine compiles under 'wasm-unsafe-eval' to create the key.
    await page.goto("/");
    await waitForDatabase(page);
    const { wrappedKey, userId } = await generateIdentity(page, PASSPHRASE);
    await putRecord(page, "keys", {
      id: "primary",
      wrappedKey,
      userId,
      displayName: "Policy Cook",
      updatedAt: Date.now(),
    });

    // Unlocking the key signs the recipe it saves.
    await page.goto("/new");
    await field(page, "Title").fill("Policy Pancakes");
    await field(page, "Makes").fill("4");
    await field(page, "Unit").fill("pancakes");
    await field(page, "Step 1").fill("Mix and fry.");
    await saveAndUnlock(page, PASSPHRASE);
    await expect(
      page.getByRole("heading", { level: 1, name: "Policy Pancakes" }),
    ).toBeVisible();
    expect(await violations(page)).toEqual([]);

    // The default server on this machine and an https server the cook adds
    // are both reachable; a plain-http server elsewhere is not.
    const local = await fakeServer(page, "http://localhost:8080");
    const remote = await fakeServer(page, "https://crisp.example.test");
    const lan = await fakeServer(page, "http://192.168.0.10:8080");
    await page.goto("/settings");
    await expect.poll(() => local.length).toBeGreaterThan(0);
    await connectTo(page, "https://crisp.example.test");
    await expect.poll(() => remote.length).toBeGreaterThan(0);
    expect(await violations(page)).toEqual([]);

    await connectTo(page, "http://192.168.0.10:8080");
    await expect
      .poll(() => violations(page))
      .toContainEqual({
        directive: "connect-src",
        blockedURI: expect.stringContaining("http://192.168.0.10:8080"),
      });
    expect(lan).toEqual([]);
  });
});
