import { defineConfig } from "@playwright/test";

const PORT = 4173;
const BASE_URL = `http://localhost:${PORT}`;

/**
 * End-to-end smoke tests against the production build (the service worker
 * only exists there). Specs live in `e2e/*.spec.ts`; `bun test` ignores them.
 */
export default defineConfig({
  testDir: "e2e",
  testMatch: "**/*.spec.ts",
  fullyParallel: true,
  forbidOnly: Boolean(process.env["CI"]),
  retries: process.env["CI"] ? 1 : 0,
  reporter: process.env["CI"] ? "github" : "list",
  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
    serviceWorkers: "allow",
  },
  webServer: {
    command: `bun run build && bun run preview --port ${PORT}`,
    url: BASE_URL,
    reuseExistingServer: !process.env["CI"],
    timeout: 120_000,
  },
  projects: [
    {
      name: "mobile",
      use: {
        browserName: "chromium",
        viewport: { width: 390, height: 844 },
        deviceScaleFactor: 3,
        isMobile: true,
        hasTouch: true,
      },
    },
    {
      name: "tablet",
      use: {
        browserName: "chromium",
        viewport: { width: 820, height: 1180 },
        deviceScaleFactor: 2,
        isMobile: true,
        hasTouch: true,
      },
    },
  ],
});
