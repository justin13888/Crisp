/**
 * The offline-first local library: a typed IndexedDB store for imported and
 * authored `.crisp` bundles and their last verification verdict.
 *
 * The browser holds the bundle bytes; the app stays fully usable offline.
 * `idb` is used rather than raw IndexedDB to keep the surface small and
 * well-tested.
 */

import { type DBSchema, type IDBPDatabase, openDB } from "idb";
import type { ComplianceReport, Kind } from "./types";

/** A bundle as held in the local library. */
export interface StoredBundle {
  /** The document id — the primary key. */
  docId: string;
  /** Whether the bundle is a recipe or a book. */
  kind: Kind;
  /** A display title, resolved at import time. */
  title: string;
  /** The raw `.crisp` file bytes. */
  bytes: Uint8Array;
  /** The verification verdict recorded at import time. */
  report: ComplianceReport;
  /** Whether this bundle has been published to a server. */
  published: boolean;
  /** When the bundle entered the local library (epoch ms). */
  importedAt: number;
}

/** The IndexedDB schema. */
interface CrispSchema extends DBSchema {
  bundles: {
    key: string;
    value: StoredBundle;
    indexes: { "by-imported": number };
  };
}

const DB_NAME = "crisp";
const DB_VERSION = 1;

let dbPromise: Promise<IDBPDatabase<CrispSchema>> | null = null;

/** Opens (and lazily creates) the IndexedDB database. */
function db(): Promise<IDBPDatabase<CrispSchema>> {
  if (!dbPromise) {
    dbPromise = openDB<CrispSchema>(DB_NAME, DB_VERSION, {
      upgrade(database) {
        const store = database.createObjectStore("bundles", {
          keyPath: "docId",
        });
        store.createIndex("by-imported", "importedAt");
      },
    });
  }
  return dbPromise;
}

/** Inserts or replaces a bundle in the local library. */
export async function saveBundle(bundle: StoredBundle): Promise<void> {
  await (await db()).put("bundles", bundle);
}

/** Fetches one bundle by document id. */
export async function getBundle(
  docId: string,
): Promise<StoredBundle | undefined> {
  return (await db()).get("bundles", docId);
}

/** Returns every stored bundle, newest import first. */
export async function allBundles(): Promise<StoredBundle[]> {
  const bundles = await (await db()).getAllFromIndex("bundles", "by-imported");
  return bundles.reverse();
}

/** Removes a bundle from the local library. */
export async function deleteBundle(docId: string): Promise<void> {
  await (await db()).delete("bundles", docId);
}
