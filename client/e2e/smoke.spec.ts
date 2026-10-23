import { expect, test } from "@playwright/test";

// The smoke suite asserts only the app shell, so it passes whether or not the
// crisp-core WASM artifacts under public/crisp-wasm/ were built.

test.describe("app shell", () => {
  test("renders the shell", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("header.topbar .brand")).toHaveText("Crisp");
    await expect(page.locator("main")).toBeVisible();
    await expect(page).toHaveTitle("Crisp");
  });

  test("shows the navigation that fits the screen", async ({ page }, info) => {
    await page.goto("/");
    const bottomNav = page.locator("nav.bottomnav");
    const topNav = page.locator("header.topbar nav");
    if (info.project.name === "mobile") {
      await expect(bottomNav).toBeVisible();
      await expect(topNav).toBeHidden();
      // Thumb-sized targets.
      const box = await bottomNav
        .getByRole("link", { name: "Import" })
        .boundingBox();
      expect(box?.height ?? 0).toBeGreaterThanOrEqual(48);
      await bottomNav.getByRole("link", { name: "Import" }).tap();
      await expect(page).toHaveURL(/\/import$/);
      await expect(bottomNav.getByRole("link", { name: "Import" })).toHaveClass(
        /active/,
      );
    } else {
      await expect(topNav).toBeVisible();
      await expect(bottomNav).toBeHidden();
    }
  });

  test("follows the dark color scheme", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "light" });
    await page.goto("/");
    const background = () =>
      page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(await background()).toBe("rgb(255, 250, 243)");
    await page.emulateMedia({ colorScheme: "dark" });
    expect(await background()).toBe("rgb(26, 22, 18)");
  });
});

test.describe("offline", () => {
  test("the service worker precaches the app so it reloads offline", async ({
    page,
    context,
  }) => {
    await page.goto("/");
    // Registration happens immediately; wait until the worker controls the
    // page (it claims clients on activation, after precaching everything).
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

    await context.setOffline(true);
    await page.reload();
    await expect(page.locator("header.topbar .brand")).toHaveText("Crisp");

    // Client-side routes also resolve offline via the cached shell.
    await page.goto("/browse");
    await expect(page.locator("header.topbar .brand")).toHaveText("Crisp");
    await context.setOffline(false);
  });
});
