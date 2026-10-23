import { expect, type Page, test } from "@playwright/test";

// Settings against the real WASM engine. No Crisp server runs in CI, so the
// server card and Browse must degrade gracefully to their offline states.

const PASSPHRASE = "correct horse battery staple";

const status = (page: Page) => page.getByTestId("identity-status");

/** Records whether an element matching `selector` ever appears. */
async function watchFor(page: Page, selector: string) {
  await page.evaluate((wanted) => {
    const scope = window as unknown as { crispSaw?: boolean };
    scope.crispSaw = false;
    new MutationObserver(() => {
      if (document.querySelector(wanted)) scope.crispSaw = true;
    }).observe(document.body, { subtree: true, childList: true });
  }, selector);
}

/** Whether the watched element appeared. */
function sawIt(page: Page): Promise<boolean> {
  return page.evaluate(
    () => (window as unknown as { crispSaw?: boolean }).crispSaw === true,
  );
}

test.describe("identity", () => {
  test("create, back up, lock, and unlock", async ({ page }) => {
    await page.goto("/settings");
    const card = page.getByRole("region", { name: "Identity" });
    await expect(card).toContainText("Your key lives only on this device.");

    await card.getByLabel("Display name").fill("Test Cook");
    await card.getByLabel("Passphrase", { exact: true }).fill("short");
    await expect(card.getByText("Too short.")).toBeVisible();
    await card.getByLabel("Passphrase", { exact: true }).fill(PASSPHRASE);
    await expect(card.getByText("Strong.")).toBeVisible();
    await card
      .getByLabel("Confirm passphrase", { exact: true })
      .fill("something else");
    await card.getByRole("button", { name: "Create identity" }).click();
    await expect(card.getByRole("alert")).toHaveText(
      "The passphrases don’t match.",
    );

    await card
      .getByLabel("Confirm passphrase", { exact: true })
      .fill(PASSPHRASE);
    // Key wrapping is slow on purpose and blocks the main thread, so the
    // spinner is recorded by an observer rather than polled.
    await watchFor(page, '[role="status"][aria-label="Creating your key"]');
    await card.getByRole("button", { name: "Create identity" }).click();
    await expect(status(page)).toHaveText("Unlocked", { timeout: 30_000 });
    expect(await sawIt(page)).toBe(true);
    await expect(
      card.getByRole("status", { name: "Creating your key" }),
    ).toHaveCount(0);
    await expect(card).toContainText("Test Cook");
    await expect(card.locator(".identity__id")).toContainText("…");

    const download = page.waitForEvent("download");
    await card.getByRole("button", { name: "Download backup" }).click();
    expect((await download).suggestedFilename()).toBe(
      "crisp-identity.crispkey",
    );

    await card.getByRole("button", { name: "Lock" }).click();
    await expect(status(page)).toHaveText("Locked");

    await card
      .getByLabel("Passphrase", { exact: true })
      .fill("wrong passphrase");
    await card.getByRole("button", { name: "Unlock" }).click();
    await expect(card.getByRole("alert")).toContainText(
      "didn’t unlock your key",
      { timeout: 30_000 },
    );
    await expect(status(page)).toHaveText("Locked");

    await card.getByLabel("Passphrase", { exact: true }).fill(PASSPHRASE);
    await card.getByRole("button", { name: "Unlock" }).click();
    await expect(status(page)).toHaveText("Unlocked", { timeout: 30_000 });

    // The identity persists across reloads, locked.
    await page.reload();
    await expect(status(page)).toHaveText("Locked");
  });

  test("forgetting needs a typed confirmation", async ({ page }) => {
    await page.goto("/settings");
    const card = page.getByRole("region", { name: "Identity" });
    await card.getByLabel("Display name").fill("Temp");
    await card.getByLabel("Passphrase", { exact: true }).fill(PASSPHRASE);
    await card
      .getByLabel("Confirm passphrase", { exact: true })
      .fill(PASSPHRASE);
    await card.getByRole("button", { name: "Create identity" }).click();
    await expect(status(page)).toHaveText("Unlocked", { timeout: 30_000 });

    await card.getByRole("button", { name: "Forget on this device…" }).click();
    const dialog = page.getByRole("dialog");
    const confirm = dialog.getByRole("button", { name: "Forget identity" });
    await expect(confirm).toBeDisabled();
    await dialog.getByLabel("Type forget to confirm").fill("forget");
    await confirm.click();
    await expect(card.getByLabel("Display name")).toBeVisible();
  });
});

test.describe("server", () => {
  test("shows the offline state when no server answers", async ({ page }) => {
    await page.goto("/settings");
    const card = page.getByRole("region", { name: "Server" });
    await expect(card.getByTestId("server-status")).toHaveAttribute(
      "data-status",
      "offline",
      { timeout: 15_000 },
    );
    await expect(card).toContainText("Can’t reach this server right now");
    await expect(card.getByLabel("Server address")).toHaveValue(/^https?:\/\//);
    await expect(card.getByRole("button", { name: "Sign in" })).toHaveCount(0);

    await card.getByLabel("Server address").fill("not a url at all");
    await card.getByRole("button", { name: "Connect" }).click();
    await expect(card.getByRole("alert")).toContainText("http");
  });

  test("Browse explains the offline state and links to Settings", async ({
    page,
  }) => {
    await page.goto("/browse");
    const offline = page.getByTestId("browse-offline");
    await expect(offline).toBeVisible({ timeout: 15_000 });
    await expect(offline).toContainText("No server connection");
    await offline.getByRole("link", { name: "Server settings" }).click();
    await expect(page).toHaveURL(/\/settings$/);
  });
});

test.describe("storage", () => {
  test("shows versions and the spec link", async ({ page }) => {
    await page.goto("/settings");
    const card = page.getByRole("region", { name: "Storage & app" });
    await expect(card).toContainText("Crisp core");
    await expect(
      card.getByRole("link", { name: "Read the Crisp format specification" }),
    ).toHaveAttribute("href", /spec/);
    await expect(card.locator("dd").nth(1)).not.toHaveText("…");
  });
});
