import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";

beforeAll(() => GlobalRegistrator.register({ url: "http://localhost/" }));
afterAll(() => GlobalRegistrator.unregister());

describe("setLocale", () => {
  test("switches the app instance, <html lang>, and the stored choice", async () => {
    const { currentLocale, i18n, setLocale, storedLocale } = await import(
      "./index"
    );
    setLocale("fr");
    expect(currentLocale()).toBe("fr");
    expect(i18n.t("nav.settings")).toBe("Réglages");
    expect(document.documentElement.lang).toBe("fr");
    expect(storedLocale()).toBe("fr");

    // Forgetting the choice follows the device again (English here).
    setLocale(null);
    expect(storedLocale()).toBeNull();
    expect(currentLocale()).toBe("en");
    expect(i18n.t("nav.settings")).toBe("Settings");
    expect(document.documentElement.lang).toBe("en");
  });
});
