import {
  afterAll,
  afterEach,
  beforeAll,
  describe,
  expect,
  test,
} from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import { NAMESPACES } from "./catalogue";
import { en } from "./en";
import { fr } from "./fr";
import { createI18n, RESOURCES } from "./instance";
import {
  FALLBACK_LOCALE,
  initialLocale,
  isLocale,
  LOCALE_NAMES,
  LOCALE_STORAGE_KEY,
  matchLocale,
  SUPPORTED_LOCALES,
  storedLocale,
  storeLocale,
} from "./locales";

/** The `{{name}}` placeholders a string interpolates, sorted. */
function placeholders(text: string): string[] {
  return [...text.matchAll(/\{\{\s*(\w+)/g)].map((m) => m[1] ?? "").sort();
}

/** A namespace's entries as plain string pairs. */
function entries(table: object): [string, string][] {
  return Object.entries(table).filter(
    (entry): entry is [string, string] => typeof entry[1] === "string",
  );
}

describe("fallback", () => {
  const partial = {
    en: { common: { greeting: "Hello", farewell: "Goodbye" } },
    fr: { common: { greeting: "Bonjour" } },
  };

  test("a key the locale lacks falls back to English", () => {
    const i18n = createI18n("fr", partial);
    expect(i18n.t("greeting" as never)).toBe("Bonjour");
    expect(i18n.t("farewell" as never)).toBe("Goodbye");
  });

  test("a whole missing namespace falls back to English", () => {
    const i18n = createI18n("fr", {
      en: { common: {}, library: { title: "Library" } },
      fr: { common: {} },
    });
    expect(i18n.t("library:title" as never)).toBe("Library");
  });

  test("a key no catalogue has renders as the key", () => {
    const i18n = createI18n("fr", partial);
    expect(i18n.t("nowhere" as never)).toBe("nowhere");
  });

  test("an unsupported language resolves to English", () => {
    const i18n = createI18n("de" as never, partial);
    expect(i18n.t("greeting" as never)).toBe("Hello");
  });
});

describe("shipped catalogues", () => {
  test("the app instance translates in the chosen locale", () => {
    expect(createI18n("en").t("nav.settings")).toBe("Settings");
    expect(createI18n("fr").t("nav.settings")).toBe("Réglages");
  });

  test("every namespace is registered in each locale", () => {
    for (const locale of SUPPORTED_LOCALES) {
      expect(Object.keys(RESOURCES[locale]).sort()).toEqual(
        [...NAMESPACES].sort(),
      );
    }
  });

  test("French translates every English key with the same placeholders", () => {
    for (const ns of NAMESPACES) {
      const french = new Map(entries(fr[ns]));
      for (const [key, english] of entries(en[ns])) {
        const translated = french.get(key);
        expect({ ns, key, has: translated !== undefined }).toEqual({
          ns,
          key,
          has: true,
        });
        expect({ ns, key, vars: placeholders(translated ?? "") }).toEqual({
          ns,
          key,
          vars: placeholders(english),
        });
      }
    }
  });

  test("plural forms pick by count", () => {
    const i18n = createI18n("fr", {
      en: {
        common: { item_one: "{{count}} item", item_other: "{{count}} items" },
      },
      fr: {
        common: {
          item_one: "{{count}} élément",
          item_other: "{{count}} éléments",
        },
      },
    });
    expect(i18n.t("item" as never, { count: 1 })).toBe("1 élément");
    expect(i18n.t("item" as never, { count: 3 })).toBe("3 éléments");
  });

  test("French plurals carry the `_many` form its plural rules select", () => {
    // Intl.PluralRules("fr") picks `many` for counts like 1 000 000; without
    // the form, i18next would fall back to the English string.
    for (const ns of NAMESPACES) {
      const french = new Set(Object.keys(fr[ns]));
      for (const key of Object.keys(en[ns])) {
        if (!key.endsWith("_other")) continue;
        const many = key.replace(/_other$/, "_many");
        expect({ ns, many, has: french.has(many) }).toEqual({
          ns,
          many,
          has: true,
        });
      }
    }
  });

  test("every locale has a display name", () => {
    for (const locale of SUPPORTED_LOCALES) {
      expect(LOCALE_NAMES[locale].length).toBeGreaterThan(0);
    }
  });
});

describe("locale choice", () => {
  beforeAll(() => GlobalRegistrator.register({ url: "http://localhost/" }));
  afterAll(() => GlobalRegistrator.unregister());
  afterEach(() => localStorage.removeItem(LOCALE_STORAGE_KEY));

  test("matches on the primary subtag, first preference first", () => {
    expect(matchLocale(["fr-CA", "en-US"])).toBe("fr");
    expect(matchLocale(["de-DE", "EN-gb"])).toBe("en");
    expect(matchLocale(["de", "ja"])).toBe(FALLBACK_LOCALE);
    expect(matchLocale([])).toBe(FALLBACK_LOCALE);
  });

  test("recognises only supported locales", () => {
    expect(isLocale("fr")).toBe(true);
    expect(isLocale("de")).toBe(false);
    expect(isLocale(undefined)).toBe(false);
  });

  test("a stored choice wins and can be forgotten", () => {
    expect(storedLocale()).toBeNull();
    storeLocale("fr");
    expect(storedLocale()).toBe("fr");
    expect(initialLocale()).toBe("fr");
    storeLocale(null);
    expect(storedLocale()).toBeNull();
  });

  test("an unrecognised stored value is ignored", () => {
    localStorage.setItem(LOCALE_STORAGE_KEY, "klingon");
    expect(storedLocale()).toBeNull();
  });
});
