import { expect, type Page, test } from "@playwright/test";

// The recipe editor end to end: author, autosave, save (unlocking the
// identity), and revise. Needs the crisp-core WASM under public/crisp-wasm/.

const PASSPHRASE = "passphrase123";

/** Seeds a locked identity, as Settings would create one. */
async function seedIdentity(page: Page): Promise<void> {
  await page.goto("/");
  // The app creates the v2 schema on first load; wait for it.
  await page.waitForFunction(async () =>
    (await indexedDB.databases()).some(
      (db) => db.name === "crisp" && db.version === 2,
    ),
  );
  await page.evaluate(async (passphrase) => {
    const url = "/crisp-wasm/crisp.js";
    const m = await import(/* @vite-ignore */ url);
    await m.default({ module_or_path: "/crisp-wasm/crisp_bg.wasm" });
    const signer = m.Signer.generate();
    const record = {
      id: "primary",
      wrappedKey: signer.wrap(passphrase),
      userId: signer.userId,
      displayName: "Test Cook",
      updatedAt: Date.now(),
    };
    await new Promise<void>((resolve, reject) => {
      const open = indexedDB.open("crisp", 2);
      open.onerror = () => reject(open.error);
      open.onsuccess = () => {
        const db = open.result;
        const tx = db.transaction("keys", "readwrite");
        tx.objectStore("keys").put(record);
        tx.oncomplete = () => {
          db.close();
          resolve();
        };
        tx.onerror = () => reject(tx.error);
      };
    });
  }, PASSPHRASE);
  await page.reload();
}

/** The stored revision of a library document. */
async function storedRevision(page: Page, docId: string): Promise<string> {
  return page.evaluate(
    (id) =>
      new Promise<string>((resolve, reject) => {
        const open = indexedDB.open("crisp", 2);
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const db = open.result;
          const get = db.transaction("bundles").objectStore("bundles").get(id);
          get.onsuccess = () => {
            db.close();
            resolve(String(get.result?.revision ?? ""));
          };
          get.onerror = () => reject(get.error);
        };
      }),
    docId,
  );
}

/** The stored autosave of the new-recipe draft, serialized. */
async function storedDraft(page: Page): Promise<string> {
  return page.evaluate(
    () =>
      new Promise<string>((resolve, reject) => {
        const open = indexedDB.open("crisp", 2);
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const db = open.result;
          try {
            const get = db
              .transaction("drafts")
              .objectStore("drafts")
              .get("new");
            get.onsuccess = () => {
              db.close();
              resolve(JSON.stringify(get.result?.state ?? null));
            };
            get.onerror = () => reject(get.error);
          } catch (error) {
            db.close();
            reject(error);
          }
        };
      }),
  );
}

/** Saves, unlocking the identity when asked; returns the new doc id. */
async function saveAndUnlock(page: Page): Promise<string> {
  await page.getByRole("button", { name: "Save to library" }).click();
  const passphrase = page.getByLabel("Passphrase to unlock your identity");
  await expect(passphrase).toBeVisible();
  await passphrase.fill(PASSPHRASE);
  await page.getByRole("button", { name: "Unlock and save" }).click();
  await expect(page).toHaveURL(/\/recipe\/rcp1[a-z0-9]+$/, { timeout: 20_000 });
  return decodeURIComponent(
    new URL(page.url()).pathname.split("/").pop() ?? "",
  );
}

const field = (page: Page, label: string) =>
  page.getByLabel(label, { exact: true });

test.describe("recipe editor", () => {
  test.beforeEach(async ({ page }) => {
    await seedIdentity(page);
  });

  test("authors and saves a recipe with groups, timers and temperature", async ({
    page,
  }) => {
    await page.goto("/new");
    await field(page, "Title").fill("E2E Pancakes");
    await field(page, "Makes").fill("4");
    await field(page, "Unit").fill("servings");
    await field(page, "Prep").fill("10 min");

    // Ingredient 1, then Enter adds and focuses the next row.
    await field(page, "Amount of ingredient 1").fill("1 1/2 cups");
    await expect(page.getByText("= 1½ cups")).toBeVisible();
    await field(page, "Ingredient 1").fill("flour");
    await field(page, "Ingredient 1").press("Enter");
    await expect(field(page, "Amount of ingredient 2")).toBeFocused();
    await page.keyboard.type("1 tsp");
    await page.keyboard.press("Tab");
    await page.keyboard.type("salt");

    // A named section with its own ingredient.
    const ingredients = page.locator("#sec-ingredients");
    await ingredients.getByRole("button", { name: "+ Add section" }).click();
    await expect(ingredients.getByLabel("Section title")).toBeFocused();
    await page.keyboard.type("For the glaze");
    await field(page, "Amount of ingredient 3").fill("1 cup");
    await field(page, "Ingredient 3").fill("icing sugar");

    // Two steps; the second with a timer and an oven temperature.
    await field(page, "Step 1").fill("Mix the flour and salt.");
    const steps = page.locator("#sec-steps");
    await steps.getByRole("button", { name: "+ Add step" }).click();
    await expect(field(page, "Step 2")).toBeFocused();
    await page.keyboard.type("Bake until golden.");
    const second = page.locator(".step-card").nth(1);
    await second.locator("summary", { hasText: "Timers" }).click();
    await second.getByRole("button", { name: "+ Add timer" }).click();
    await second.getByLabel("Timer 1 duration").fill("20 min");
    await second.getByLabel("Timer 1 label").fill("bake");
    await second.locator("summary", { hasText: "Heat" }).click();
    await second.getByLabel("Temperature", { exact: true }).fill("180");
    await second.getByLabel("Of the").selectOption("oven");

    // Step 1 uses the flour.
    const first = page.locator(".step-card").nth(0);
    await first.locator("summary", { hasText: "Ingredients used" }).click();
    await first.getByLabel("1 1/2 cups flour").check();

    const docId = await saveAndUnlock(page);
    expect(docId).toMatch(/^rcp1/);
    expect(await storedRevision(page, docId)).toMatch(/^rev1/);
  });

  test("shows problems and keeps the save blocked until fixed", async ({
    page,
  }) => {
    await page.goto("/new");
    await page.getByRole("button", { name: "Save to library" }).click();
    const summary = page.locator("#editor-problems");
    await expect(summary).toContainText("to fix before saving");
    await summary
      .getByRole("button", { name: /^Title:/ })
      .first()
      .click();
    await expect(field(page, "Title")).toBeFocused();
    await expect(field(page, "Title")).toHaveAttribute("aria-invalid", "true");
    await expect(page).toHaveURL(/\/new$/);
  });

  test("autosaves and restores an unsaved draft", async ({ page }) => {
    await page.goto("/new");
    await field(page, "Title").fill("Autosaved soup");
    await field(page, "Ingredient 1").fill("water");
    await expect(page.getByText("Draft saved")).toBeVisible();
    // Wait until the stored draft holds the latest keystrokes, not just the
    // first autosave.
    await expect.poll(() => storedDraft(page)).toContain("water");
    await page.reload();
    await expect(
      page.getByText("Restored your unsaved draft — discard?"),
    ).toBeVisible();
    await expect(field(page, "Title")).toHaveValue("Autosaved soup");
    await expect(field(page, "Ingredient 1")).toHaveValue("water");

    await page.getByRole("button", { name: "Discard draft" }).click();
    await expect(field(page, "Title")).toHaveValue("");
    await page.reload();
    await expect(field(page, "Title")).toHaveValue("");
    await expect(page.getByText("Restored your unsaved draft")).toBeHidden();
  });

  test("editing a saved recipe makes a new revision of the same document", async ({
    page,
  }) => {
    await page.goto("/new");
    await field(page, "Title").fill("Revisable stew");
    await field(page, "Makes").fill("2");
    await field(page, "Unit").fill("bowls");
    await field(page, "Amount of ingredient 1").fill("500 g");
    await field(page, "Ingredient 1").fill("beef");
    await field(page, "Step 1").fill("Simmer for hours.");
    const docId = await saveAndUnlock(page);
    const firstRevision = await storedRevision(page, docId);

    await page.goto(`/edit/${docId}`);
    await expect(field(page, "Title")).toHaveValue("Revisable stew");
    await expect(field(page, "Amount of ingredient 1")).toHaveValue("500 g");
    await field(page, "Title").fill("Revisable stew, improved");
    const again = await saveAndUnlock(page);

    expect(again).toBe(docId);
    const secondRevision = await storedRevision(page, docId);
    expect(secondRevision).toMatch(/^rev1/);
    expect(secondRevision).not.toBe(firstRevision);
  });
});
