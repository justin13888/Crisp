import { describe, expect, test } from "bun:test";
import type { ComplianceReport } from "../../lib/crisp";
import {
  EMPTY_FILTERS,
  facetCount,
  facetLabel,
  hasFilters,
  mayHaveMore,
  pageRequest,
  toggleFacet,
} from "./browseModel";
import {
  describeTally,
  type ImportDeps,
  importAll,
  importOne,
  tallyOutcomes,
} from "./importFiles";
import {
  draftLink,
  draftTitle,
  entryTitle,
  filterEntries,
  foldText,
  isLibrarySort,
  type ListableEntry,
  relativeTime,
  removedFromState,
  sortEntries,
  trustDisplay,
} from "./libraryModel";

function entry(
  docId: string,
  title: string,
  updatedAt: number,
  authors: string[] = [],
): ListableEntry {
  return { record: { docId, title, updatedAt }, summary: { title, authors } };
}

const ENTRIES = [
  entry("r1", "Crème brûlée", 300, ["Ada"]),
  entry("r2", "apple pie", 100, ["Grace Hopper"]),
  entry("r3", "Banana bread", 200),
  entry("r4", "Recipe 10", 50),
  entry("r5", "Recipe 9", 50),
];

describe("library search", () => {
  test("folds accents and case", () => {
    expect(foldText("Crème BRÛLÉE")).toBe("creme brulee");
  });

  test("an empty query keeps everything, in order", () => {
    expect(filterEntries(ENTRIES, "  ").map((e) => e.record.docId)).toEqual([
      "r1",
      "r2",
      "r3",
      "r4",
      "r5",
    ]);
  });

  test("matches every word against title and authors", () => {
    expect(filterEntries(ENTRIES, "creme").map((e) => e.record.docId)).toEqual([
      "r1",
    ]);
    expect(filterEntries(ENTRIES, "pie hopper").length).toBe(1);
    expect(filterEntries(ENTRIES, "pie ada").length).toBe(0);
    expect(filterEntries(ENTRIES, "BREAD")[0]?.record.docId).toBe("r3");
  });

  test("falls back to the stored title, then Untitled", () => {
    expect(
      entryTitle({
        record: { docId: "x", title: "Stored", updatedAt: 0 },
        summary: { title: " ", authors: [] },
      }),
    ).toBe("Stored");
    expect(
      entryTitle({
        record: { docId: "x", title: "", updatedAt: 0 },
        summary: { title: "", authors: [] },
      }),
    ).toBe("Untitled");
  });
});

describe("library sort", () => {
  test("recent puts the newest first, ties by id", () => {
    expect(sortEntries(ENTRIES, "recent").map((e) => e.record.docId)).toEqual([
      "r1",
      "r3",
      "r2",
      "r4",
      "r5",
    ]);
  });

  test("title is case-insensitive and numeric-aware", () => {
    expect(sortEntries(ENTRIES, "title").map((e) => e.summary.title)).toEqual([
      "apple pie",
      "Banana bread",
      "Crème brûlée",
      "Recipe 9",
      "Recipe 10",
    ]);
  });

  test("does not mutate its input", () => {
    const copy = [...ENTRIES];
    sortEntries(ENTRIES, "title");
    expect(ENTRIES).toEqual(copy);
  });

  test("narrows sort values", () => {
    expect(isLibrarySort("recent")).toBe(true);
    expect(isLibrarySort("title")).toBe(true);
    expect(isLibrarySort("cooked")).toBe(false);
    expect(isLibrarySort(null)).toBe(false);
  });
});

describe("trust and drafts", () => {
  test("labels every trust level", () => {
    expect(trustDisplay("endorsed")).toMatchObject({
      label: "Endorsed",
      tone: "ok",
    });
    expect(trustDisplay("author-signed").label).toBe("Signed");
    expect(trustDisplay("unsigned")).toMatchObject({
      label: "Unsigned",
      tone: "warn",
    });
  });

  test("links drafts to the editor", () => {
    expect(draftLink("new")).toBe("/new");
    expect(draftLink("edit:rcpABC")).toBe("/edit/rcpABC");
    expect(draftLink("edit:")).toBeNull();
    expect(draftLink("something")).toBeNull();
  });

  test("finds a draft title when the state has one", () => {
    expect(draftTitle({ title: " Soup " }, "new")).toBe("Soup");
    expect(draftTitle({ name: "Stew" }, "edit:x")).toBe("Stew");
    expect(draftTitle(null, "new")).toBe("New recipe");
    expect(draftTitle({ title: 3 }, "edit:x")).toBe("Edited recipe");
  });
});

describe("removedFromState", () => {
  const record = {
    docId: "rcp1",
    kind: "recipe",
    title: "Soup",
    revision: "rev1",
    bytes: new Uint8Array([1]),
    report: {},
    published: false,
    importedAt: 1,
    updatedAt: 2,
  };

  test("accepts a stored record", () => {
    expect(removedFromState({ removed: record })).toEqual({
      ...record,
      kind: "recipe",
    });
  });

  test("rejects anything else", () => {
    expect(removedFromState(null)).toBeNull();
    expect(removedFromState({})).toBeNull();
    expect(removedFromState({ removed: { ...record, bytes: [1] } })).toBeNull();
    expect(removedFromState({ removed: { ...record, kind: "x" } })).toBeNull();
    expect(removedFromState({ removed: { ...record, docId: "" } })).toBeNull();
  });
});

describe("relativeTime", () => {
  const now = Date.UTC(2026, 5, 15, 12);
  test("covers the ranges", () => {
    expect(relativeTime(now - 10_000, now)).toBe("just now");
    expect(relativeTime(now - 5 * 60_000, now)).toBe("5 min ago");
    expect(relativeTime(now - 3 * 3_600_000, now)).toBe("3 h ago");
    expect(relativeTime(now - 30 * 3_600_000, now)).toBe("yesterday");
    expect(relativeTime(now - 4 * 86_400_000, now)).toBe("4 days ago");
    expect(relativeTime(now + 5000, now)).toBe("just now");
  });

  test("shows a date for older times, with the year when it differs", () => {
    expect(relativeTime(Date.UTC(2026, 0, 2, 12), now, "en-US")).toBe("Jan 2");
    expect(relativeTime(Date.UTC(2024, 0, 2, 12), now, "en-US")).toBe(
      "Jan 2, 2024",
    );
  });
});

function report(
  compliant: boolean,
  checks: ComplianceReport["checks"],
): ComplianceReport {
  return {
    crisp_report: 1,
    compliant,
    class: "structural",
    format_version: 1,
    trust: "unsigned",
    verified_at: 0,
    checks,
  };
}

/** A fake library: files whose first byte is 1 are accepted. */
function fakeDeps(existing: Set<string> = new Set()): ImportDeps {
  return {
    add: async (bytes) => {
      if (bytes[0] !== 1) throw new Error("This file does not verify: bad id");
      const docId = `rcp${bytes[1] ?? 0}`;
      const replaced = existing.has(docId);
      existing.add(docId);
      return {
        entry: {
          record: { docId },
          summary: { title: `Recipe ${docId}` },
          report: { trust: "author-signed" },
        },
        replaced,
      };
    },
    inspect: async (bytes) => {
      if (bytes[0] === 9) throw new Error("not CBOR");
      return {
        title: "Broken",
        report: report(false, [
          { id: "A1", status: "pass" },
          {
            id: "C3",
            status: "fail",
            code: "E.ID.MISMATCH",
            detail: "id does not match",
          },
          { id: "D1", status: "skipped" },
        ]),
      };
    },
  };
}

describe("import pipeline", () => {
  test("stores compliant files", async () => {
    const outcome = await importOne(
      { name: "a.crisp", bytes: new Uint8Array([1, 7]) },
      fakeDeps(),
    );
    expect(outcome).toEqual({
      status: "added",
      name: "a.crisp",
      docId: "rcp7",
      title: "Recipe rcp7",
      trust: "author-signed",
    });
  });

  test("reports replacements as updates", async () => {
    const outcome = await importOne(
      { name: "a.crisp", bytes: new Uint8Array([1, 7]) },
      fakeDeps(new Set(["rcp7"])),
    );
    expect(outcome.status).toBe("updated");
  });

  test("refusals carry the reason and the failed checks", async () => {
    const outcome = await importOne(
      { name: "bad.crisp", bytes: new Uint8Array([2]) },
      fakeDeps(),
    );
    expect(outcome).toEqual({
      status: "failed",
      name: "bad.crisp",
      reason: "This file does not verify: bad id",
      title: "Broken",
      failedChecks: [
        { id: "C3", code: "E.ID.MISMATCH", detail: "id does not match" },
      ],
    });
  });

  test("unopenable files fail without checks", async () => {
    const outcome = await importOne(
      { name: "junk.txt", bytes: new Uint8Array([9]) },
      fakeDeps(),
    );
    expect(outcome).toMatchObject({
      status: "failed",
      failedChecks: [],
      title: undefined,
    });
  });

  test("imports a batch in order, reporting progress", async () => {
    const seen: string[] = [];
    const outcomes = await importAll(
      [
        { name: "1", bytes: new Uint8Array([1, 1]) },
        { name: "2", bytes: new Uint8Array([2]) },
        { name: "3", bytes: new Uint8Array([1, 1]) },
      ],
      fakeDeps(),
      (outcome, index) => seen.push(`${index}:${outcome.status}`),
    );
    expect(seen).toEqual(["0:added", "1:failed", "2:updated"]);
    const tally = tallyOutcomes(outcomes);
    expect(tally).toEqual({ added: 1, updated: 1, failed: 1 });
    expect(describeTally(tally)).toBe("1 added, 1 updated, 1 failed");
    expect(describeTally(tallyOutcomes([]))).toBe("Nothing imported");
  });
});

describe("browse model", () => {
  test("toggles facets without mutating", () => {
    const one = toggleFacet(EMPTY_FILTERS, "diet", "vegan");
    expect(one.diet).toEqual(["vegan"]);
    expect(EMPTY_FILTERS.diet).toEqual([]);
    expect(toggleFacet(one, "diet", "vegan").diet).toEqual([]);
    expect(facetCount(toggleFacet(one, "course", "main"))).toBe(2);
  });

  test("browses when unfiltered, searches otherwise", () => {
    expect(hasFilters({ ...EMPTY_FILTERS, q: "   " })).toBe(false);
    expect(pageRequest(EMPTY_FILTERS, 40)).toEqual({
      kind: "browse",
      limit: 20,
      offset: 40,
    });
    expect(
      pageRequest(
        { q: " soup ", course: [], diet: ["vegan"], allergen_free: ["milk"] },
        0,
        10,
      ),
    ).toEqual({
      kind: "search",
      query: {
        q: "soup",
        diet: ["vegan"],
        allergen_free: ["milk"],
        limit: 10,
        offset: 0,
      },
    });
  });

  test("pages and labels", () => {
    expect(mayHaveMore(20)).toBe(true);
    expect(mayHaveMore(3)).toBe(false);
    expect(facetLabel("tree-nuts")).toBe("Tree nuts");
    expect(facetLabel("gluten-free")).toBe("Gluten free");
  });
});
