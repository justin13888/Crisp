import { afterEach, describe, expect, test } from "bun:test";
import { openDB } from "idb";
import { startTimer } from "../timers";
import {
  type BundleRecord,
  type CrispStore,
  cookKey,
  DB_VERSION,
  openStore,
} from "./index";
import { requestPersistence, resetPersistenceRequest } from "./persistence";
import { migrateV1Bundle } from "./schema";

let counter = 0;
const opened: CrispStore[] = [];

/** Opens a fresh, uniquely named database. */
async function fresh(): Promise<CrispStore> {
  const store = await openStore(`test-${Date.now()}-${counter++}`);
  opened.push(store);
  return store;
}

afterEach(() => {
  for (const store of opened.splice(0)) store.db.close();
});

function bundle(docId: string, updatedAt: number): BundleRecord {
  return {
    docId,
    kind: "recipe",
    title: `Recipe ${docId}`,
    revision: "rev1",
    bytes: new Uint8Array([1, 2, 3]),
    report: { compliant: true },
    published: false,
    importedAt: updatedAt,
    updatedAt,
  };
}

describe("schema", () => {
  test("creates every v2 store", async () => {
    const store = await fresh();
    expect(store.db.version).toBe(DB_VERSION);
    expect([...store.db.objectStoreNames].sort()).toEqual([
      "bundles",
      "cook",
      "drafts",
      "keys",
      "servers",
      "timers",
    ]);
  });

  test("upgrades a v1 database, preserving bundle records", async () => {
    const name = `legacy-${Date.now()}`;
    const v1 = await openDB(name, 1, {
      upgrade(db) {
        const store = db.createObjectStore("bundles", { keyPath: "docId" });
        store.createIndex("by-imported", "importedAt");
      },
    });
    await v1.put("bundles", {
      docId: "doc-a",
      kind: "book",
      title: "Old book",
      bytes: new Uint8Array([9, 9]),
      report: { compliant: false },
      published: true,
      importedAt: 1000,
    });
    await v1.put("bundles", {
      docId: "doc-b",
      kind: "recipe",
      title: "Old recipe",
      bytes: new Uint8Array([7]),
      report: { compliant: true },
      published: false,
      importedAt: 2000,
    });
    v1.close();

    const store = await openStore(name);
    opened.push(store);
    expect(store.db.version).toBe(2);
    const a = await store.bundles.get("doc-a");
    expect(a).toEqual({
      docId: "doc-a",
      kind: "book",
      title: "Old book",
      revision: "",
      bytes: new Uint8Array([9, 9]),
      report: { compliant: false },
      published: true,
      importedAt: 1000,
      updatedAt: 1000,
    });
    expect((await store.bundles.all()).map((b) => b.docId)).toEqual([
      "doc-b",
      "doc-a",
    ]);
    // The new stores are usable straight away.
    await store.drafts.put({ id: "d", state: {}, updatedAt: 1 });
    expect(await store.drafts.get("d")).toBeDefined();
  });

  test("migrateV1Bundle back-fills and rejects keyless values", () => {
    expect(migrateV1Bundle(null)).toBeNull();
    expect(migrateV1Bundle({ title: "x" })).toBeNull();
    expect(migrateV1Bundle({ docId: "x" })).toEqual({
      docId: "x",
      kind: "recipe",
      title: "",
      revision: "",
      bytes: new Uint8Array(),
      report: undefined,
      published: false,
      importedAt: 0,
      updatedAt: 0,
    });
  });
});

describe("bundles", () => {
  test("CRUD with newest-updated-first listing", async () => {
    const { bundles } = await fresh();
    await bundles.put(bundle("a", 10));
    await bundles.put(bundle("b", 30));
    await bundles.put(bundle("c", 20));
    expect((await bundles.all()).map((b) => b.docId)).toEqual(["b", "c", "a"]);
    const a = await bundles.get("a");
    expect(a?.bytes).toEqual(new Uint8Array([1, 2, 3]));
    await bundles.put({ ...bundle("a", 40), revision: "rev2" });
    expect((await bundles.get("a"))?.revision).toBe("rev2");
    expect((await bundles.all())[0]?.docId).toBe("a");
    await bundles.delete("a");
    expect(await bundles.get("a")).toBeUndefined();
    await bundles.delete("missing");
  });
});

describe("drafts", () => {
  test("stores opaque editor state", async () => {
    const { drafts } = await fresh();
    await drafts.put({ id: "1", state: { title: "Soup" }, updatedAt: 1 });
    await drafts.put({ id: "2", state: ["x"], updatedAt: 2 });
    expect((await drafts.all()).map((d) => d.id)).toEqual(["2", "1"]);
    expect((await drafts.get("1"))?.state).toEqual({ title: "Soup" });
    await drafts.delete("1");
    expect(await drafts.all()).toHaveLength(1);
  });
});

describe("cook", () => {
  test("keys progress by document and revision", async () => {
    const { cook } = await fresh();
    const base = {
      docId: "doc",
      currentStep: 2,
      checkedIngredients: [0, 3],
      checkedSteps: [0, 1],
      scale: { num: 3, den: 2 },
      unitSystem: "metric" as const,
      updatedAt: 5,
    };
    await cook.put({ ...base, revision: "r1" });
    await cook.put({ ...base, revision: "r2", currentStep: 0 });
    await cook.put({ ...base, docId: "other", revision: "r1" });
    const r1 = await cook.get("doc", "r1");
    expect(r1?.key).toBe(cookKey("doc", "r1"));
    expect(r1?.key).toBe("doc@r1");
    expect(r1?.scale).toEqual({ num: 3, den: 2 });
    expect((await cook.forDoc("doc")).map((r) => r.revision).sort()).toEqual([
      "r1",
      "r2",
    ]);
    await cook.delete("doc", "r1");
    expect(await cook.get("doc", "r1")).toBeUndefined();
  });
});

describe("timers", () => {
  test("persists, validates and atomically replaces timers", async () => {
    const { timers, db } = await fresh();
    const ctx = { title: "Stew" };
    const a = startTimer(
      [],
      { id: "a", label: "a", context: ctx, durationMs: 1000 },
      0,
    ).timer;
    const b = startTimer(
      [],
      { id: "b", label: "b", context: ctx, durationMs: 2000 },
      0,
    ).timer;
    await timers.put(a);
    await timers.put(b);
    expect((await timers.all()).map((t) => t.id).sort()).toEqual(["a", "b"]);

    // A corrupted record is dropped on read, not trusted.
    await db.put("timers", { ...a, id: "bad", state: "bogus" as "running" });
    expect((await timers.all()).map((t) => t.id).sort()).toEqual(["a", "b"]);

    await timers.replaceAll([b]);
    expect((await timers.all()).map((t) => t.id)).toEqual(["b"]);
    await timers.delete("b");
    expect(await timers.all()).toEqual([]);
  });
});

describe("keys and servers", () => {
  test("round-trip records", async () => {
    const { keys, servers } = await fresh();
    await keys.put({
      id: "k1",
      wrappedKey: new Uint8Array([4, 5]),
      userId: "usr1abc",
      displayName: "Ada",
      updatedAt: 1,
    });
    expect((await keys.get("k1"))?.wrappedKey).toEqual(new Uint8Array([4, 5]));
    expect(await keys.all()).toHaveLength(1);
    await keys.delete("k1");
    expect(await keys.all()).toEqual([]);

    await servers.put({
      url: "https://crisp.example",
      platformKeys: { pk: "x" },
      token: "t",
      username: "ada",
      updatedAt: 1,
    });
    await servers.put({
      url: "https://a.example",
      platformKeys: null,
      updatedAt: 2,
    });
    expect((await servers.all()).map((s) => s.url)).toEqual([
      "https://a.example",
      "https://crisp.example",
    ]);
    expect((await servers.get("https://crisp.example"))?.token).toBe("t");
    await servers.delete("https://a.example");
    expect(await servers.all()).toHaveLength(1);
  });
});

describe("requestPersistence", () => {
  afterEach(resetPersistenceRequest);

  test("asks once and memoizes the answer", async () => {
    let calls = 0;
    const storage = {
      persisted: async () => false,
      persist: async () => {
        calls++;
        return true;
      },
    };
    expect(await requestPersistence(storage)).toBe(true);
    expect(await requestPersistence(storage)).toBe(true);
    expect(calls).toBe(1);
  });

  test("skips the prompt when already persistent", async () => {
    let asked = false;
    const result = await requestPersistence({
      persisted: async () => true,
      persist: async () => {
        asked = true;
        return true;
      },
    });
    expect(result).toBe(true);
    expect(asked).toBe(false);
  });

  test("tolerates missing support and failures", async () => {
    expect(await requestPersistence(undefined)).toBe(false);
    resetPersistenceRequest();
    expect(await requestPersistence({})).toBe(false);
    resetPersistenceRequest();
    expect(
      await requestPersistence({
        persist: async () => {
          throw new Error("denied");
        },
      }),
    ).toBe(false);
  });
});
