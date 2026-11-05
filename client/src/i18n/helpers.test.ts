import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { facetLabel } from "../components/library/browseModel";
import { describeTally } from "../components/library/importFiles";
import {
  entryTitle,
  relativeTime,
  SORT_OPTIONS,
} from "../components/library/libraryModel";
import {
  timerRangeHint,
  trustInfo,
  UNIT_OPTIONS,
} from "../components/recipe/model";
import {
  formatBytes,
  passphraseProblem,
  passphraseStrength,
} from "../components/settings/settingsModel";
import { formatDoneAgo } from "../lib/timeFormat";
import { cookAnnouncement } from "../routes/cook/cookView";
import { locatePointer } from "../routes/editor/issues";
import { en } from "./en";
import { i18n } from "./index";

// Helpers that build interface text read the app-wide instance at call
// time, so switching the language switches what they return.

const NBSP = " ";
const noIndex = {
  ingredients: [7],
  equipment: [],
  steps: [3],
  ingredientGroups: [],
  stepGroups: [],
};

describe("helpers in French", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("fr");
  });
  afterEach(async () => {
    await i18n.changeLanguage("en");
  });

  test("library sort labels, untitled fallback, and relative times", () => {
    expect(SORT_OPTIONS.map((o) => o.label)).toEqual(["Récentes", "A–Z"]);
    expect(
      entryTitle({
        record: { docId: "d", title: " ", updatedAt: 0 },
        summary: { title: "", authors: [] },
      }),
    ).toBe("Sans titre");
    const now = Date.UTC(2026, 5, 15, 12);
    expect(relativeTime(now, now)).toBe("à l’instant");
    expect(relativeTime(now - 5 * 60_000, now)).toBe("il y a 5 min");
    expect(relativeTime(now - 4 * 86_400_000, now)).toBe("il y a 4 jours");
  });

  test("the import tally agrees in number", () => {
    expect(describeTally({ added: 1, updated: 2, failed: 0 })).toBe(
      "1 ajoutée, 2 mises à jour",
    );
    expect(describeTally({ added: 0, updated: 0, failed: 0 })).toBe(
      "Rien n’a été importé",
    );
  });

  test("registry facets are translated; other codes are capitalized", () => {
    expect(facetLabel("tree-nuts")).toBe("Fruits à coque");
    expect(facetLabel("main")).toBe("Plat principal");
    expect(facetLabel("smoked-fish")).toBe("Smoked fish");
  });

  test("passphrase strength, problems, and byte sizes", () => {
    expect(passphraseStrength("abcdefg")).toMatchObject({
      label: "Trop courte",
      hint: "Encore 1 caractère.",
    });
    expect(passphraseStrength("abcde").hint).toBe("Encore 3 caractères.");
    expect(passphraseStrength("correct horse battery staple").label).toBe(
      "Robuste",
    );
    expect(passphraseProblem("long enough", "other")).toBe(
      "Les phrases secrètes ne correspondent pas.",
    );
    expect(formatBytes(1536)).toBe(`1,5${NBSP}Ko`);
    expect(formatBytes(1023)).toBe(`1023${NBSP}o`);
  });

  test("recipe units, trust badges, and the ranged-timer hint", () => {
    expect(UNIT_OPTIONS.map((o) => o.label)).toEqual([
      "D’origine",
      "Métriques",
      "US",
    ]);
    expect(trustInfo("author-signed")).toMatchObject({
      label: "Signée par l’auteur",
      shortLabel: "Signée",
      tone: "ok",
      shortTone: "",
    });
    const timer = {
      label: "cuire",
      seconds: 300,
      max_seconds: 420,
      text: "5-7 min",
      passive: false,
    };
    const hint = timerRangeHint(timer, (s) => `${s / 60} min`);
    expect(hint).toBe("Vérifier à 5 min · prêt à 7 min");
  });

  test("timer done-ago text and the cook announcement", () => {
    expect(formatDoneAgo(0)).toBe("Terminé à l’instant");
    expect(formatDoneAgo(125 * 60_000)).toBe("Terminé il y a 2 h 5 min");
    const steps = [{ number: 1, text: [], section: "Pâte" }];
    expect(
      cookAnnouncement(
        { phase: "steps", currentStep: 0 },
        steps,
        () => "Pétrir",
      ),
    ).toBe("Étape 1 sur 1. Pâte. Pétrir");
    expect(
      cookAnnouncement({ phase: "done", currentStep: 0 }, [], () => ""),
    ).toBe("C’est terminé.");
  });

  test("editor problem labels", () => {
    expect(locatePointer("/recipes/0/ingredients/0/quantity", noIndex)).toEqual(
      { field: "f-ing-7-amount", label: "Ingrédient 1 · quantité" },
    );
    expect(locatePointer("/recipes/0/steps/0/timers/1", noIndex).label).toBe(
      "Étape 1 · minuteur 2",
    );
    expect(locatePointer("/recipes/0/times/prep", noIndex).label).toBe(
      "Temps de préparation",
    );
    expect(locatePointer("/nowhere", noIndex).label).toBe("Recette");
  });
});

describe("helpers follow a language change", () => {
  test("a label read after switching is in the new language", async () => {
    const option = SORT_OPTIONS[0];
    expect(option?.label).toBe("Recent");
    await i18n.changeLanguage("fr");
    try {
      expect(option?.label).toBe("Récentes");
    } finally {
      await i18n.changeLanguage("en");
    }
    expect(option?.label).toBe("Recent");
  });
});

// The format's CDDL lives at the repository root, outside `client/`. Walk up
// from this file rather than hardcoding a depth, so the test still finds it
// from a copied sandbox (e.g. Stryker's `client/.stryker-tmp/sandbox-*/`).
function findSpec(from: string): string {
  let dir = from;
  for (;;) {
    const candidate = join(dir, "spec", "crisp-v1.cddl");
    if (existsSync(candidate)) return candidate;
    const parent = dirname(dir);
    if (parent === dir) throw new Error(`spec/crisp-v1.cddl not above ${from}`);
    dir = parent;
  }
}

describe("facet catalogue", () => {
  test("names every registry code the format lists", async () => {
    const cddl = await Bun.file(findSpec(import.meta.dir)).text();
    const codes = ["course-code", "diet-code", "allergen-code"].flatMap(
      (rule) => {
        const rulePattern = new RegExp(
          `^${rule}\\s*=([\\s\\S]*?)\\n\\s*\\n`,
          "m",
        );
        const body = cddl.match(rulePattern)?.[1] ?? "";
        return [...body.matchAll(/"([a-z0-9-]+)"/g)].map((m) => m[1] ?? "");
      },
    );
    expect(codes.length).toBeGreaterThan(30);
    const named = new Set(Object.keys(en.browse));
    expect(codes.filter((code) => !named.has(`facet.${code}`))).toEqual([]);
  });
});
