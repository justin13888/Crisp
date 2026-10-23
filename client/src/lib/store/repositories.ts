/**
 * Typed repositories, one per object store.
 */

import { createLogger } from "../log";
import { parseTimers, type Timer } from "../timers";
import {
  type BundleRecord,
  type CookRecord,
  type CrispDB,
  cookKey,
  type DraftRecord,
  type KeyRecord,
  type ServerRecord,
} from "./schema";

const log = createLogger("store");

/** Basic keyed CRUD over one store. */
export interface Repository<K, V> {
  /** Fetches one record. */
  get(key: K): Promise<V | undefined>;
  /** Inserts or replaces a record. */
  put(value: V): Promise<void>;
  /** Every record (order documented per repository). */
  all(): Promise<V[]>;
  /** Removes a record (no-op if absent). */
  delete(key: K): Promise<void>;
}

/** Library bundles; `all()` is newest-updated first. */
export type BundleRepository = Repository<string, BundleRecord>;

/** Editor drafts; `all()` is newest-updated first. */
export type DraftRepository = Repository<string, DraftRecord>;

/** Server records; `all()` is in URL order. */
export type ServerRepository = Repository<string, ServerRecord>;

/** Author keys; `all()` is in id order. */
export type KeyRepository = Repository<string, KeyRecord>;

/** Cook-mode progress, keyed by document and revision. */
export interface CookRepository {
  /** Progress for one revision. */
  get(docId: string, revision: string): Promise<CookRecord | undefined>;
  /** Saves progress (the key is derived from `docId` and `revision`). */
  put(record: Omit<CookRecord, "key">): Promise<void>;
  /** Every saved session for a document, any revision. */
  forDoc(docId: string): Promise<CookRecord[]>;
  /** Removes progress for one revision. */
  delete(docId: string, revision: string): Promise<void>;
}

/** Persisted timers (validated on read). */
export interface TimerRepository {
  /** Every well-formed stored timer. */
  all(): Promise<Timer[]>;
  /** Inserts or replaces one timer. */
  put(timer: Timer): Promise<void>;
  /** Removes one timer. */
  delete(id: string): Promise<void>;
  /** Atomically replaces the whole set. */
  replaceAll(timers: readonly Timer[]): Promise<void>;
}

/** All repositories over one open database. */
export interface CrispStore {
  /** The open database. */
  db: CrispDB;
  /** Library bundles. */
  bundles: BundleRepository;
  /** Editor drafts. */
  drafts: DraftRepository;
  /** Cook-mode progress. */
  cook: CookRepository;
  /** Kitchen timers. */
  timers: TimerRepository;
  /** Wrapped author keys. */
  keys: KeyRepository;
  /** Known servers. */
  servers: ServerRepository;
}

/** Builds the repositories over an open database. */
export function createRepositories(db: CrispDB): CrispStore {
  return {
    db,
    bundles: {
      get: (docId) => db.get("bundles", docId),
      put: async (record) => {
        await db.put("bundles", record);
        log.debug("bundle.put", {
          docId: record.docId,
          revision: record.revision,
          bytes: record.bytes.byteLength,
        });
      },
      all: async () =>
        (await db.getAllFromIndex("bundles", "by-updated")).reverse(),
      delete: async (docId) => {
        await db.delete("bundles", docId);
        log.info("bundle.delete", { docId });
      },
    },
    drafts: {
      get: (id) => db.get("drafts", id),
      put: async (record) => {
        await db.put("drafts", record);
        log.debug("draft.put", { id: record.id });
      },
      all: async () =>
        (await db.getAllFromIndex("drafts", "by-updated")).reverse(),
      delete: async (id) => {
        await db.delete("drafts", id);
        log.debug("draft.delete", { id });
      },
    },
    cook: {
      get: (docId, revision) => db.get("cook", cookKey(docId, revision)),
      put: async (record) => {
        const key = cookKey(record.docId, record.revision);
        await db.put("cook", { ...record, key });
        log.trace("cook.put", { key, step: record.currentStep });
      },
      forDoc: (docId) => db.getAllFromIndex("cook", "by-doc", docId),
      delete: async (docId, revision) => {
        await db.delete("cook", cookKey(docId, revision));
      },
    },
    timers: {
      all: async () => parseTimers(await db.getAll("timers")),
      put: async (timer) => {
        await db.put("timers", timer);
      },
      delete: async (id) => {
        await db.delete("timers", id);
      },
      replaceAll: async (timers) => {
        const tx = db.transaction("timers", "readwrite");
        await tx.store.clear();
        for (const timer of timers) await tx.store.put(timer);
        await tx.done;
        log.trace("timers.replace", { count: timers.length });
      },
    },
    keys: {
      get: (id) => db.get("keys", id),
      put: async (record) => {
        await db.put("keys", record);
        log.info("key.put", { id: record.id, userId: record.userId });
      },
      all: () => db.getAll("keys"),
      delete: async (id) => {
        await db.delete("keys", id);
        log.info("key.delete", { id });
      },
    },
    servers: {
      get: (url) => db.get("servers", url),
      put: async (record) => {
        await db.put("servers", record);
        log.info("server.put", { url: record.url });
      },
      all: () => db.getAll("servers"),
      delete: async (url) => {
        await db.delete("servers", url);
        log.info("server.delete", { url });
      },
    },
  };
}
