import { readFileSync } from "node:fs";
import { expect, type Page, test } from "@playwright/test";

// The whole dogfooding loop through the real UI and the real engine:
// create an identity, write a recipe, export the .crisp file, delete it,
// import the exported file back, cook from it with a timer, and keep
// cooking with the network gone.

const PASSPHRASE = "journey passphrase";

const field = (page: Page, label: string) =>
  page.getByLabel(label, { exact: true });

/** Waits until the service worker controls the page. */
async function waitForServiceWorker(page: Page): Promise<void> {
  await page.waitForFunction(
    async () => {
      const registration = await navigator.serviceWorker.ready;
      return (
        registration.active !== null &&
        navigator.serviceWorker.controller !== null
      );
    },
    undefined,
    { timeout: 15_000 },
  );
}

test("author, export, re-import, and cook offline", async ({
  page,
  context,
}) => {
  test.setTimeout(120_000);

  // --- identity
  await page.goto("/settings");
  const identity = page.getByRole("region", { name: "Identity" });
  await identity.getByLabel("Display name").fill("Journey Cook");
  await identity.getByLabel("Passphrase", { exact: true }).fill(PASSPHRASE);
  await identity
    .getByLabel("Confirm passphrase", { exact: true })
    .fill(PASSPHRASE);
  await identity.getByRole("button", { name: "Create identity" }).click();
  await expect(identity.getByRole("button", { name: "Lock" })).toBeVisible({
    timeout: 20_000,
  });

  // --- author (navigating reloads the app, which locks the identity again)
  await page.goto("/new");
  await field(page, "Title").fill("Weeknight Tomato Soup");
  await field(page, "Makes").fill("4");
  await field(page, "Unit").fill("bowls");
  await field(page, "Amount of ingredient 1").fill("800 g");
  await field(page, "Ingredient 1").fill("canned tomatoes");
  await field(page, "Step 1").fill("Simmer the tomatoes.");
  const step = page.locator(".step-card").nth(0);
  await step.locator("summary", { hasText: "Timers" }).click();
  await step.getByRole("button", { name: "+ Add timer" }).click();
  await step.getByLabel("Timer 1 duration").fill("15 min");
  await step.getByLabel("Timer 1 label").fill("simmer");
  await page.getByRole("button", { name: "Save to library" }).click();
  await page.getByLabel("Passphrase to unlock your identity").fill(PASSPHRASE);
  await page.getByRole("button", { name: "Unlock and save" }).click();
  await expect(page).toHaveURL(/\/recipe\/rcp1[a-z0-9]+$/, { timeout: 20_000 });
  const docId = decodeURIComponent(
    new URL(page.url()).pathname.split("/").pop() ?? "",
  );
  await expect(
    page.getByRole("heading", { level: 1, name: "Weeknight Tomato Soup" }),
  ).toBeVisible();

  // --- export the atomic .crisp file
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export" }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe("weeknight-tomato-soup.crisp");
  const exported = readFileSync(await file.path());
  // The self-describe tag that opens every .crisp file.
  expect([...exported.subarray(0, 3)]).toEqual([0xd9, 0xd9, 0xf7]);

  // --- delete it, then import the exported file back
  await page.getByRole("button", { name: "Delete" }).click();
  await page
    .getByRole("alertdialog", { name: "Delete recipe" })
    .getByRole("button", { name: "Delete" })
    .click();
  await expect(page).toHaveURL(/\/$/);
  // Only the undo toast still names it; no card links to it.
  await expect(
    page.getByRole("link", { name: /Weeknight Tomato Soup/ }),
  ).toHaveCount(0, { timeout: 10_000 });
  await page.getByTestId("crisp-file-input").setInputFiles({
    name: "weeknight-tomato-soup.crisp",
    mimeType: "application/crisp+cbor",
    buffer: exported,
  });
  await page
    .getByRole("link", { name: /Weeknight Tomato Soup/ })
    .first()
    .click();
  await expect(page).toHaveURL(new RegExp(`/recipe/${docId}$`));

  // --- cook with a timer
  await waitForServiceWorker(page);
  await page.getByRole("link", { name: "Start cooking" }).click();
  await page
    .getByRole("navigation", { name: "Cook navigation" })
    .getByRole("button", { name: "Start cooking" })
    .click();
  await expect(
    page.getByRole("heading", { level: 1, name: "Step 1 of 1" }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Start simmer timer/i }).click();
  const countdown = page.locator(".cook-timers [role=timer]");
  await expect(countdown).toHaveText(/^(15:00|14:\d\d)$/);

  // --- the network goes away mid-recipe: everything keeps working
  await context.setOffline(true);
  await page.reload();
  await expect(
    page.getByRole("heading", { level: 1, name: "Step 1 of 1" }),
  ).toBeVisible();
  await expect(countdown).toHaveText(/^14:\d\d$/);
  await context.setOffline(false);
});
