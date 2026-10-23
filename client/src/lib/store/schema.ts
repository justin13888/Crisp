/**
 * The local IndexedDB schema (version 2) and its upgrade path.
 *
 * Record shapes are deliberately minimal and self-contained: values that come
 * from the WASM boundary (compliance reports, editor state, platform keys)
 * are stored as `unknown` and must be re-validated by their consumers.
 */

import type { DBSchema, IDBPDatabase, IDBPTransaction, StoreNames } from "idb";
import type { Scale, UnitSystem } from "../cookSession";
import { createLogger } from "../log";
import type { Timer } from "../timers";

const log = createLogger("store");

/** The database name (shared with the v1 library store). */
export const DB_NAME = "crisp";

/** The current schema version. */
export const DB_VERSION = 2;

/** Whether a bundle is a recipe or a book. */
export type BundleKind = "recipe" | "book";

/** A `.crisp` bundle in the local library. */
export interface BundleRecord {
  /** The document id — primary key. */
  docId: string;
  /** Recipe or book. */
  kind: BundleKind;
  /** Display title resolved at import time. */
  title: string;
  /** The revision hash held locally (`""` for records migrated from v1). */
  revision: string;
  /** The raw `.crisp` file bytes. */
  bytes: Uint8Array;
  /** The last verification report — untrusted until narrowed by the caller. */
  report: unknown;
  /** Whether the bundle has been published to a server. */
  published: boolean;
  /** When the bundle entered the library (epoch ms). */
  importedAt: number;
  /** When the record last changed (epoch ms). */
  updatedAt: number;
}

/** An unsaved editor draft. */
export interface DraftRecord {
  /** Draft id — primary key. */
  id: string;
  /** Opaque editor state (JSON-compatible). */
  state: unknown;
  /** When the draft last changed (epoch ms). */
  updatedAt: number;
}

/** Saved cook-mode progress for one recipe revision. */
export interface CookRecord {
  /** `${docId}@${revision}` — primary key (see {@link cookKey}). */
  key: string;
  /** The document id. */
  docId: string;
  /** The revision cooked. */
  revision: string;
  /** Cook phase, when known. */
  phase?: "mise" | "steps" | "done";
  /** The 0-based step in view. */
  currentStep: number;
  /** Checked ingredient indexes. */
  checkedIngredients: number[];
  /** Completed step indexes. */
  checkedSteps: number[];
  /** Serving scale. */
  scale: Scale;
  /** Display unit system. */
  unitSystem: UnitSystem;
  /** When progress last changed (epoch ms). */
  updatedAt: number;
}

/** A locally held, passphrase-wrapped author key. */
export interface KeyRecord {
  /** Key id — primary key. */
  id: string;
  /** The wrapped key file bytes. */
  wrappedKey: Uint8Array;
  /** The `usr…` id the key belongs to. */
  userId: string;
  /** The display name bound to the key. */
  displayName: string;
  /** When the record last changed (epoch ms). */
  updatedAt: number;
}

/** A known Crisp server. */
export interface ServerRecord {
  /** Server base URL — primary key. */
  url: string;
  /** Pinned platform public keys — untrusted until narrowed. */
  platformKeys: unknown;
  /** Session token, if signed in. */
  token?: string;
  /** Username on that server, if signed in. */
  username?: string;
  /** When the record last changed (epoch ms). */
  updatedAt: number;
}

/** The typed v2 schema. */
export interface CrispSchema extends DBSchema {
  bundles: {
    key: string;
    value: BundleRecord;
    indexes: { "by-imported": number; "by-updated": number };
  };
  drafts: {
    key: string;
    value: DraftRecord;
    indexes: { "by-updated": number };
  };
  cook: {
    key: string;
    value: CookRecord;
    indexes: { "by-doc": string };
  };
  timers: {
    key: string;
    value: Timer;
  };
  keys: {
    key: string;
    value: KeyRecord;
  };
  servers: {
    key: string;
    value: ServerRecord;
  };
}

/** An open v2 database handle. */
export type CrispDB = IDBPDatabase<CrispSchema>;

/** The primary key of a cook record. */
export function cookKey(docId: string, revision: string): string {
  return `${docId}@${revision}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/** Fills the fields v1 bundle records lack. */
export function migrateV1Bundle(value: unknown): BundleRecord | null {
  if (!isRecord(value) || typeof value["docId"] !== "string") return null;
  const importedAt =
    typeof value["importedAt"] === "number" ? value["importedAt"] : 0;
  const kind = value["kind"] === "book" ? "book" : "recipe";
  const bytes = value["bytes"];
  return {
    docId: value["docId"],
    kind,
    title: typeof value["title"] === "string" ? value["title"] : "",
    revision: typeof value["revision"] === "string" ? value["revision"] : "",
    bytes: bytes instanceof Uint8Array ? bytes : new Uint8Array(),
    report: value["report"],
    published: value["published"] === true,
    importedAt,
    updatedAt:
      typeof value["updatedAt"] === "number" ? value["updatedAt"] : importedAt,
  };
}

/**
 * The `openDB` upgrade callback: creates every store from scratch, or
 * upgrades a v1 database (a lone `bundles` store) in place, preserving and
 * back-filling its records.
 */
export function upgradeSchema(
  db: CrispDB,
  oldVersion: number,
  tx: IDBPTransaction<CrispSchema, StoreNames<CrispSchema>[], "versionchange">,
): Promise<void> | undefined {
  log.info("db.upgrade", { from: oldVersion, to: DB_VERSION });
  if (oldVersion < 1) {
    const bundles = db.createObjectStore("bundles", { keyPath: "docId" });
    bundles.createIndex("by-imported", "importedAt");
  }
  if (oldVersion >= 2) return undefined;

  const bundles = tx.objectStore("bundles");
  if (!bundles.indexNames.contains("by-updated")) {
    bundles.createIndex("by-updated", "updatedAt");
  }
  db.createObjectStore("drafts", { keyPath: "id" }).createIndex(
    "by-updated",
    "updatedAt",
  );
  db.createObjectStore("cook", { keyPath: "key" }).createIndex(
    "by-doc",
    "docId",
  );
  db.createObjectStore("timers", { keyPath: "id" });
  db.createObjectStore("keys", { keyPath: "id" });
  db.createObjectStore("servers", { keyPath: "url" });

  if (oldVersion < 1) return undefined;
  return (async () => {
    let migrated = 0;
    let dropped = 0;
    let cursor = await bundles.openCursor();
    while (cursor) {
      const record = migrateV1Bundle(cursor.value);
      if (record) {
        await cursor.update(record);
        migrated++;
      } else {
        await cursor.delete();
        dropped++;
      }
      cursor = await cursor.continue();
    }
    log.info("db.upgrade.bundles-migrated", { migrated, dropped });
  })();
}
