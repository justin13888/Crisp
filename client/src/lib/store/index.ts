/**
 * The offline-first local store: IndexedDB schema v2 over `idb`.
 *
 * Use {@link getStore} for the app-wide singleton, or {@link openStore} with
 * a custom name for isolated instances (tests).
 */

import { openDB } from "idb";
import { createLogger } from "../log";
import { type CrispStore, createRepositories } from "./repositories";
import { type CrispSchema, DB_NAME, DB_VERSION, upgradeSchema } from "./schema";

export { requestPersistence } from "./persistence";
export type {
  BundleRepository,
  CookRepository,
  CrispStore,
  DraftRepository,
  KeyRepository,
  Repository,
  ServerRepository,
  TimerRepository,
} from "./repositories";
export { createRepositories } from "./repositories";
export type {
  BundleKind,
  BundleRecord,
  CookRecord,
  CrispDB,
  DraftRecord,
  KeyRecord,
  ServerRecord,
} from "./schema";
export { cookKey, DB_NAME, DB_VERSION } from "./schema";

const log = createLogger("store");

/** Opens (creating or upgrading) a Crisp database. */
export async function openStore(name: string = DB_NAME): Promise<CrispStore> {
  const started = performance.now();
  const db = await openDB<CrispSchema>(name, DB_VERSION, {
    upgrade: (database, oldVersion, _newVersion, tx) =>
      upgradeSchema(database, oldVersion, tx),
    blocked: (currentVersion) => {
      log.warn("db.blocked", { name, currentVersion, wanted: DB_VERSION });
    },
    blocking: () => {
      // A newer tab wants to upgrade: step aside so it is not stuck.
      log.warn("db.blocking", { name });
      db.close();
      if (singleton && name === DB_NAME) singleton = null;
    },
    terminated: () => {
      log.error("db.terminated", { name });
      if (name === DB_NAME) singleton = null;
    },
  });
  log.debug("db.open", {
    name,
    version: db.version,
    ms: Math.round(performance.now() - started),
  });
  return createRepositories(db);
}

let singleton: Promise<CrispStore> | null = null;

/** The app-wide store, opened lazily once. */
export function getStore(): Promise<CrispStore> {
  if (!singleton) {
    singleton = openStore(DB_NAME).catch((error: unknown) => {
      singleton = null;
      log.error("db.open.failed", { error: String(error) });
      throw error;
    });
  }
  return singleton;
}
