/**
 * The offline-first local library — compatibility adapter.
 *
 * The database itself now lives in `lib/store/` (schema v2, which upgrades
 * the original v1 `bundles` store in place). This module keeps the original
 * `StoredBundle` API working for existing screens until they move to
 * `getStore().bundles` directly. Opening the v1 schema here would fail with a
 * `VersionError` once v2 exists, so everything routes through the store.
 */

import { createLogger } from "./log";
import { type BundleRecord, getStore } from "./store";
import type { ComplianceReport, Kind } from "./types";

const log = createLogger("db");

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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/** A structural check on a stored report (it was validated at import). */
function isReport(value: unknown): value is ComplianceReport {
  return (
    isRecord(value) &&
    typeof value["compliant"] === "boolean" &&
    typeof value["trust"] === "string" &&
    typeof value["class"] === "string" &&
    Array.isArray(value["checks"])
  );
}

/** Narrows a v2 record to the legacy shape, or `null` if malformed. */
function toStored(record: BundleRecord): StoredBundle | null {
  const report = record.report;
  if (!isReport(report)) {
    log.warn("bundle.report.malformed", { docId: record.docId });
    return null;
  }
  return {
    docId: record.docId,
    kind: record.kind,
    title: record.title,
    bytes: record.bytes,
    report,
    published: record.published,
    importedAt: record.importedAt,
  };
}

/** Inserts or replaces a bundle in the local library. */
export async function saveBundle(bundle: StoredBundle): Promise<void> {
  const store = await getStore();
  const existing = await store.bundles.get(bundle.docId);
  await store.bundles.put({
    ...bundle,
    // The legacy API does not carry the revision; keep any known value.
    revision: existing?.revision ?? "",
    updatedAt: Date.now(),
  });
}

/** Fetches one bundle by document id. */
export async function getBundle(
  docId: string,
): Promise<StoredBundle | undefined> {
  const record = await (await getStore()).bundles.get(docId);
  return (record && toStored(record)) ?? undefined;
}

/** Returns every stored bundle, newest import first. */
export async function allBundles(): Promise<StoredBundle[]> {
  const { db } = await getStore();
  const records = await db.getAllFromIndex("bundles", "by-imported");
  return records
    .reverse()
    .map(toStored)
    .filter((b): b is StoredBundle => b !== null);
}

/** Removes a bundle from the local library. */
export async function deleteBundle(docId: string): Promise<void> {
  await (await getStore()).bundles.delete(docId);
}
