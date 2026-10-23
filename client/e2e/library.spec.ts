import { fileURLToPath } from "node:url";
import { expect, type Page, test } from "@playwright/test";

// The library against real compliance-suite files, verified by the WASM
// engine in the browser. Every test starts with an empty IndexedDB.

const suite = (file: string) =>
  fileURLToPath(new URL(`../../spec/compliance/${file}`, import.meta.url));

const FULL = suite("valid/full-recipe.crisp");
const MINIMAL = suite("valid/minimal.crisp");
const INVALID = suite("invalid/E.ID.MISMATCH/id-mismatch.crisp");

const cards = (page: Page) => page.locator("article.recipe-card");

async function importFiles(page: Page, files: string[]) {
  await page.getByTestId("crisp-file-input").setInputFiles(files);
}

test.describe("library", () => {
  test("starts empty with a way in", async ({ page }) => {
    await page.goto("/");
    await expect(
      page.getByRole("heading", { name: "Your cookbook starts here" }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Create your first recipe" }),
    ).toHaveAttribute("href", "/new");
    await expect(
      page.getByRole("button", { name: "Import a .crisp file" }),
    ).toBeVisible();
  });

  test("imports, searches, deletes, and undoes", async ({ page }) => {
    await page.goto("/");
    await expect(
      page.getByRole("heading", { name: "Your cookbook starts here" }),
    ).toBeVisible();

    await importFiles(page, [FULL, MINIMAL]);
    const results = page.getByRole("region", { name: "Import results" });
    await expect(results.getByRole("heading")).toHaveText("2 added");
    await expect(results.getByRole("link", { name: "Open" })).toHaveCount(2);
    await expect(cards(page)).toHaveCount(2);
    await expect(cards(page).first().locator(".trust-badge")).toBeVisible();

    // Search narrows to one card, and clearing brings both back.
    const titles = await page.locator(".recipe-card__title").allTextContents();
    expect(titles).toHaveLength(2);
    const [first = "", second = ""] = titles;
    const target = first.toLowerCase().includes(second.toLowerCase())
      ? first
      : second;
    const search = page.getByRole("searchbox", { name: "Search recipes" });
    await search.fill(target);
    await expect(cards(page)).toHaveCount(1);
    await expect(cards(page).locator(".recipe-card__title")).toHaveText(target);
    await search.fill("zzz-no-such-recipe");
    await expect(cards(page)).toHaveCount(0);
    await expect(page.getByText("No recipes match")).toBeVisible();
    await page.getByRole("button", { name: "Clear search" }).click();
    await expect(cards(page)).toHaveCount(2);

    // A–Z sort is remembered.
    await page.getByRole("combobox", { name: "Sort by" }).selectOption("title");
    const sorted = [...titles].sort((a, b) =>
      a.localeCompare(b, undefined, { sensitivity: "base" }),
    );
    await expect(page.locator(".recipe-card__title")).toHaveText(sorted);

    // Delete with confirmation, then undo.
    await page
      .getByRole("button", { name: `More actions for ${target}` })
      .click();
    await page.getByRole("menuitem", { name: "Delete…" }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toContainText(`Delete “${target}”?`);
    await dialog.getByRole("button", { name: "Delete" }).click();
    await expect(cards(page)).toHaveCount(1);
    const toast = page.locator(".undo-toast");
    await expect(toast).toContainText("Deleted");
    await toast.getByRole("button", { name: "Undo" }).click();
    await expect(cards(page)).toHaveCount(2);
    await expect(toast).toBeHidden();

    // Survives a reload (stored in IndexedDB).
    await page.reload();
    await expect(cards(page)).toHaveCount(2);
  });

  test("re-importing a file reports an update", async ({ page }) => {
    await page.goto("/");
    await importFiles(page, [MINIMAL]);
    await expect(cards(page)).toHaveCount(1);
    await importFiles(page, [MINIMAL]);
    await expect(page.locator(".import-row[data-status=updated]")).toHaveCount(
      1,
    );
    await expect(cards(page)).toHaveCount(1);
  });

  test("refuses a file that does not verify", async ({ page }) => {
    await page.goto("/");
    await importFiles(page, [INVALID]);
    const results = page.getByRole("region", { name: "Import results" });
    await expect(results.getByRole("heading")).toHaveText("1 failed");
    await expect(results.locator(".import-row__reason")).toContainText(
      "does not verify",
    );
    await expect(cards(page)).toHaveCount(0);
    await expect(
      page.getByRole("heading", { name: "Your cookbook starts here" }),
    ).toBeVisible();
  });

  test("the import screen shows each file's verification", async ({ page }) => {
    await page.goto("/import");
    await expect(
      page.getByRole("heading", { name: "Import recipes" }),
    ).toBeVisible();
    await importFiles(page, [MINIMAL, INVALID]);
    const results = page.getByRole("region", { name: "Import results" });
    await expect(results.getByRole("heading")).toHaveText("1 added, 1 failed");
    await expect(results.getByText("All checks passed")).toBeVisible();
    await expect(results.locator("table.checks")).toContainText(
      "E.ID.MISMATCH",
    );
    const open = results.getByRole("link", { name: "Open" });
    await expect(open).toHaveAttribute("href", /^\/recipe\/.+/);
  });
});
