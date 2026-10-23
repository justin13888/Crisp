import {
  afterAll,
  beforeEach,
  describe,
  expect,
  mock,
  setSystemTime,
  test,
} from "bun:test";
import * as crisp from "./crisp";
import type { BundleSummary, ComplianceReport, Engine } from "./crisp";
import { getStore } from "./store";

// `addFile` reads its verdict from the engine; a fake engine stands in for
// the WASM module so each test chooses the summary and report it returns.

interface Verdict {
  docId: string;
  revision: string;
  report: ComplianceReport;
}

let verdict: Verdict;

const fakeEngine = {
  Bundle: {
    open: () => ({
      verify: () => verdict.report,
      summary: () =>
        ({
          doc_id: verdict.docId,
          kind: "recipe",
          title: `Recipe ${verdict.docId}`,
          recipes: [{ revision: verdict.revision }],
        }) as unknown as BundleSummary,
      free: () => {},
    }),
  },
} as unknown as Engine;

mock.module("./crisp", () => ({
  ...crisp,
  loadEngine: async () => fakeEngine,
  preferredLanguages: () => ["en"],
}));

const { addFile, fileName } = await import("./library");

function report(overrides: Partial<ComplianceReport> = {}): ComplianceReport {
  return {
    crisp_report: 1,
    compliant: true,
    class: "structural",
    format_version: 1,
    trust: "author-signed",
    verified_at: 0,
    checks: [],
    ...overrides,
  };
}

const BYTES = new Uint8Array([1, 2, 3]);

beforeEach(async () => {
  const store = await getStore();
  for (const record of await store.bundles.all()) {
    await store.bundles.delete(record.docId);
  }
  verdict = { docId: "rcp1a", revision: "rev1a", report: report() };
});

afterAll(() => {
  setSystemTime();
});

describe("addFile", () => {
  test("stores a new file as imported now and unpublished", async () => {
    setSystemTime(new Date(1_000));
    const { entry, replaced } = await addFile(BYTES);
    expect(replaced).toBe(false);
    expect(entry.record).toMatchObject({
      docId: "rcp1a",
      revision: "rev1a",
      published: false,
      importedAt: 1_000,
      updatedAt: 1_000,
    });
    const stored = await (await getStore()).bundles.get("rcp1a");
    expect(stored?.bytes).toEqual(BYTES);
  });

  test("a re-import keeps the first importedAt and moves updatedAt", async () => {
    setSystemTime(new Date(1_000));
    await addFile(BYTES);
    setSystemTime(new Date(5_000));
    verdict = { ...verdict, revision: "rev1b" };
    const { entry, replaced } = await addFile(BYTES);
    expect(replaced).toBe(true);
    expect(entry.record).toMatchObject({
      revision: "rev1b",
      importedAt: 1_000,
      updatedAt: 5_000,
    });
  });

  test("a re-import keeps a published copy published", async () => {
    const store = await getStore();
    await addFile(BYTES);
    const first = await store.bundles.get("rcp1a");
    if (!first) throw new Error("not stored");
    await store.bundles.put({ ...first, published: true });

    const { entry } = await addFile(BYTES);
    expect(entry.report.trust).toBe("author-signed");
    expect(entry.record.published).toBe(true);
    expect((await store.bundles.get("rcp1a"))?.published).toBe(true);
  });

  test("an endorsed file is published", async () => {
    verdict = { ...verdict, report: report({ trust: "endorsed" }) };
    const { entry } = await addFile(BYTES);
    expect(entry.record.published).toBe(true);
  });

  test("refuses a non-compliant file with the failed check, storing nothing", async () => {
    verdict = {
      ...verdict,
      report: report({
        compliant: false,
        checks: [
          { id: "A1", status: "pass" },
          { id: "C3", status: "fail", detail: "bad id" },
        ] as ComplianceReport["checks"],
      }),
    };
    await expect(addFile(BYTES)).rejects.toThrow(
      "This file does not verify: bad id",
    );
    expect(await (await getStore()).bundles.get("rcp1a")).toBeUndefined();
  });
});

describe("fileName", () => {
  test("slugs the title into a .crisp name", () => {
    expect(fileName("Weeknight Tomato Soup")).toBe(
      "weeknight-tomato-soup.crisp",
    );
  });

  test("drops diacritics", () => {
    expect(fileName("Crème Brûlée")).toBe("creme-brulee.crisp");
  });

  test("collapses and trims separators", () => {
    expect(fileName("  --Mac & Cheese!!  ")).toBe("mac-cheese.crisp");
  });

  test("falls back to recipe when nothing is left", () => {
    expect(fileName("")).toBe("recipe.crisp");
    expect(fileName("!!!")).toBe("recipe.crisp");
    expect(fileName("寿司")).toBe("recipe.crisp");
  });

  test("caps the slug at 80 characters", () => {
    expect(fileName("a".repeat(200))).toBe(`${"a".repeat(80)}.crisp`);
  });
});
