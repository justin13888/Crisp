import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";

// The recipe page and cook mode against the real engine, seeded with the
// compliance-suite fixture: a 5-step brown-butter cookie recipe (sections,
// a ranged timer, passive timers, an oven temperature, a heat level).

const FIXTURE = join(
  import.meta.dirname,
  "../../spec/compliance/valid/full-recipe.crisp",
);

/** The engine module's shape (the generated declarations). */
type Engine = typeof import("../src/generated/crisp");

interface Seeded {
  docId: string;
  revision: string;
  title: string;
}

/** Imports the fixture into the library's IndexedDB store, as Import would. */
async function seedLibrary(page: Page): Promise<Seeded> {
  const bytes = [...readFileSync(FIXTURE)];
  await page.goto("/");
  // The app opens (and upgrades) the `crisp` v2 database on start.
  await page.waitForFunction(async () => {
    const dbs = await indexedDB.databases();
    return dbs.some((db) => db.name === "crisp" && db.version === 2);
  });
  return page.evaluate(async (raw) => {
    const data = new Uint8Array(raw);
    // A variable specifier: resolved by the browser, not by the bundler.
    const url = "/crisp-wasm/crisp.js";
    const m: Engine = await import(url);
    await m.default({ module_or_path: "/crisp-wasm/crisp_bg.wasm" });
    const bundle = m.Bundle.open(data);
    const summary = bundle.summary(["en"]);
    bundle.free();
    const seeded = {
      docId: summary.doc_id,
      revision: summary.recipes[0]?.revision ?? "",
      title: summary.title,
    };
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const req = indexedDB.open("crisp", 2);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction("bundles", "readwrite");
      const now = Date.now();
      tx.objectStore("bundles").put({
        docId: seeded.docId,
        kind: "recipe",
        title: seeded.title,
        revision: seeded.revision,
        bytes: data,
        report: null,
        published: false,
        importedAt: now,
        updatedAt: now,
      });
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
    db.close();
    return seeded;
  }, bytes);
}

/** The saved cook record as "step:checked-count", or null. */
async function savedProgress(
  page: Page,
  seeded: Seeded,
): Promise<string | null> {
  return page.evaluate(async ({ docId, revision }) => {
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const req = indexedDB.open("crisp", 2);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    const record = await new Promise<
      { currentStep: number; checkedIngredients: number[] } | undefined
    >((resolve, reject) => {
      const req = db
        .transaction("cook")
        .objectStore("cook")
        .get(`${docId}@${revision}`);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    db.close();
    return record
      ? `${record.currentStep}:${record.checkedIngredients.length}`
      : null;
  }, seeded);
}

const ingredientRow = (page: Page, name: string) =>
  page.locator("li.ingredient").filter({ hasText: name });

const stepHeading = (page: Page) =>
  page.getByRole("heading", { level: 1, name: /^Step \d+ of \d+/ });

test.describe("recipe page and cook mode", () => {
  test("scale, cook, persist across reload, and finish", async ({ page }) => {
    const seeded = await seedLibrary(page);

    // --- recipe page: scale and units re-render through the engine.
    await page.goto(`/recipe/${seeded.docId}`);
    await expect(
      page.getByRole("heading", { level: 1, name: seeded.title }),
    ).toBeVisible();
    await expect(page.getByText("Signed by author")).toBeVisible();
    const flour = ingredientRow(page, "all-purpose flour");
    await expect(flour).toContainText("2¼ cups");
    await page.getByRole("button", { name: "2×", exact: true }).click();
    await expect(flour).toContainText("4½ cups");
    await expect(page.getByText("48 cookies").first()).toBeVisible();
    await page.getByRole("button", { name: "Metric", exact: true }).click();
    await expect(flour).toContainText("1.1 L");
    await expect(flour).not.toContainText("cups");

    // --- mise en place keeps the scale and units.
    await page.getByRole("link", { name: "Start cooking" }).click();
    await expect(page).toHaveURL(new RegExp(`/cook/${seeded.docId}$`));
    await expect(
      page.getByRole("heading", { level: 1, name: "Mise en place" }),
    ).toBeVisible();
    await expect(ingredientRow(page, "all-purpose flour")).toContainText(
      "1.1 L",
    );
    const chocolate = page
      .locator(".cook-mise")
      .getByRole("checkbox", { name: /dark chocolate/ });
    await ingredientRow(page, "dark chocolate").click();
    await expect(chocolate).toBeChecked();
    await expect(page.getByText("1 of 8 ready")).toBeVisible();

    // --- steps by button and by keyboard.
    await page
      .getByRole("navigation", { name: "Cook navigation" })
      .getByRole("button", { name: "Start cooking" })
      .click();
    await expect(stepHeading(page)).toHaveText(/Step 1 of 5/);
    await expect(page.locator(".step-badge--heat")).toBeVisible();
    await expect(
      page.getByText(/Check at 5 min · done by 7 min/),
    ).toBeVisible();
    await page.getByRole("button", { name: "Next step" }).click();
    await expect(stepHeading(page)).toHaveText(/Step 2 of 5/);
    await page.keyboard.press("ArrowRight");
    await expect(stepHeading(page)).toHaveText(/Step 3 of 5/);
    await page.keyboard.press("ArrowLeft");
    await expect(stepHeading(page)).toHaveText(/Step 2 of 5/);
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Space");
    await expect(stepHeading(page)).toHaveText(/Step 4 of 5/);

    // --- a step ingredient checkbox and a timer.
    const stepChocolate = page
      .locator(".cook-step-ingredients")
      .getByRole("checkbox", { name: /dark chocolate/ });
    await stepChocolate.click();
    await expect(stepChocolate).toBeChecked();
    await page.getByRole("button", { name: /Start Chill timer/ }).click();
    const dock = page.getByRole("region", { name: "Kitchen timers" });
    await expect(dock).toContainText("Chill");
    await expect(page.locator(".cook-timers [role=timer]")).toHaveText(
      /^(30:00|29:\d\d)$/,
    );

    // --- reload: same step, same checks, timer still running.
    await expect.poll(() => savedProgress(page, seeded)).toBe("3:2");
    await page.reload();
    await expect(stepHeading(page)).toHaveText(/Step 4 of 5/);
    await expect(stepChocolate).toBeChecked();
    await expect(dock).toContainText("Chill");
    await expect(page.locator(".cook-timers [role=timer]")).toHaveText(
      /^29:\d\d$/,
    );
    await page.getByRole("button", { name: "Steps" }).click();
    const drawer = page.getByRole("dialog", { name: "Steps" });
    await expect(drawer).toBeVisible();
    await drawer.getByRole("button", { name: "Mise en place" }).click();
    await expect(chocolate).toBeChecked();

    // --- jump back via the drawer, start a timer by number key, finish.
    await page.getByRole("button", { name: "Steps" }).click();
    await drawer.getByRole("button", { name: /Step 5/ }).click();
    await expect(stepHeading(page)).toHaveText(/Step 5 of 5/);
    await expect(
      page.locator(".step-badge--temp", { hasText: "180°C oven" }),
    ).toBeVisible();
    await page.keyboard.press("1");
    await expect(dock).toContainText("Bake");
    await page.getByRole("button", { name: "Finish" }).click();
    await expect(
      page.getByRole("heading", { level: 1, name: "All done!" }),
    ).toBeVisible();
    await page.getByRole("link", { name: "Back to recipe" }).click();
    await expect(page).toHaveURL(new RegExp(`/recipe/${seeded.docId}$`));
    await expect(page.getByRole("link", { name: "Cook again" })).toBeVisible();
  });

  test("an unknown recipe shows a friendly not-found page", async ({
    page,
  }) => {
    await page.goto("/recipe/rcp1doesnotexist");
    await expect(
      page.getByRole("heading", { name: "Recipe not found" }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Back to your library" }),
    ).toBeVisible();
  });
});

// Design review captures: `SCREENSHOT_DIR=/tmp/shots bunx playwright test
// e2e/cook.spec.ts`. Skipped otherwise.
test.describe("screenshots", () => {
  const dir = process.env["SCREENSHOT_DIR"];
  test.skip(!dir, "set SCREENSHOT_DIR to capture screenshots");

  for (const scheme of ["light", "dark"] as const) {
    test(`cook mode (${scheme})`, async ({ page }, info) => {
      const shot = (name: string) =>
        join(dir ?? "", `${info.project.name}-${scheme}-${name}.png`);
      await page.emulateMedia({ colorScheme: scheme });
      const seeded = await seedLibrary(page);
      await page.goto(`/recipe/${seeded.docId}`);
      await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
      await page.screenshot({ path: shot("recipe"), fullPage: true });
      await page.getByRole("link", { name: "Start cooking" }).click();
      await ingredientRow(page, "all-purpose flour").click();
      await ingredientRow(page, "unsalted butter").click();
      await page.screenshot({ path: shot("mise") });
      await page
        .getByRole("navigation", { name: "Cook navigation" })
        .getByRole("button", { name: "Start cooking" })
        .click();
      await expect(stepHeading(page)).toHaveText(/Step 1 of 5/);
      await page.screenshot({ path: shot("step1") });
      await page
        .getByRole("button", { name: /Start Brown butter timer/ })
        .click();
      await page.screenshot({ path: shot("step1-timer") });
      await page.getByRole("button", { name: "Steps" }).click();
      await page.screenshot({ path: shot("drawer") });
      await page.keyboard.press("Escape");
      for (let i = 0; i < 4; i++) await page.keyboard.press("ArrowRight");
      await page.screenshot({ path: shot("step5") });
      await page.setViewportSize({
        width: Math.max(page.viewportSize()?.height ?? 0, 0),
        height: page.viewportSize()?.width ?? 0,
      });
      await page.screenshot({ path: shot("step5-landscape") });
      await page.keyboard.press("ArrowRight");
      await page.screenshot({ path: shot("done") });
    });
  }
});
