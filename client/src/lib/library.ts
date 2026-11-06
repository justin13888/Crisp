/**
 * The local library: every recipe is one atomic `.crisp` file, stored as-is
 * in IndexedDB together with its last verification report. Import, save,
 * export, and delete all move whole files.
 */

import {
  type Bundle,
  type BundleSummary,
  type ComplianceReport,
  loadEngine,
  nowSeconds,
  preferredLanguages,
} from "./crisp";
import { isRecord } from "./isRecord";
import { createLogger } from "./log";
import { type TrustedServer, trustedServers } from "./pins";
import { type BundleRecord, getStore } from "./store";

const log = createLogger("library");

/** The file extension and media type of a Crisp file. */
export const CRISP_EXTENSION = ".crisp";
/** The media type of a Crisp file. */
export const CRISP_MEDIA_TYPE = "application/crisp+cbor";

/** A stored file, opened and verified. */
export interface LibraryEntry {
  /** The stored record. */
  record: BundleRecord;
  /** A fresh summary in the reader's language. */
  summary: BundleSummary;
  /** A fresh verification report. */
  report: ComplianceReport;
}

/** What happened to an imported file. */
export interface ImportResult {
  /** The stored entry. */
  entry: LibraryEntry;
  /** Whether it replaced an existing copy of the same document. */
  replaced: boolean;
}

/** The failure code of an endorsement its server revoked (spec §8.8). */
export const REVOKED = "E.SIG.REVOKED";

/** Whether `report` says the endorsing server withdrew its endorsement. */
export function endorsementWithdrawn(report: ComplianceReport): boolean {
  return report.checks.some(
    (check) => check.status === "fail" && check.code === REVOKED,
  );
}

/**
 * Verifies an opened file against every pinned server key, then — when it
 * is endorsed — against each server's own keys and held revocations, so a
 * server can withdraw only the endorsements its own keys made (spec §8.8).
 */
export function verifyTrusted(
  bundle: Pick<Bundle, "verify" | "verifyWithRevocations">,
  servers: TrustedServer[],
  now: number,
): ComplianceReport {
  const report = bundle.verify(
    servers.flatMap((server) => server.keys),
    now,
  );
  if (report.trust !== "endorsed") return report;
  for (const { keys, revocations } of servers) {
    if (revocations.length === 0) continue;
    const checked = bundle.verifyWithRevocations(keys, revocations, now);
    if (endorsementWithdrawn(checked)) return checked;
  }
  return report;
}

/** Opens and verifies file bytes against what this device trusts. */
export async function inspect(bytes: Uint8Array): Promise<{
  bundle: Bundle;
  summary: BundleSummary;
  report: ComplianceReport;
}> {
  const engine = await loadEngine();
  const bundle = engine.Bundle.open(bytes);
  const report = verifyTrusted(bundle, await trustedServers(), nowSeconds());
  const summary = bundle.summary(preferredLanguages());
  if (endorsementWithdrawn(report)) {
    log.info("endorsement.withdrawn", { docId: summary.doc_id });
  }
  return { bundle, summary, report };
}

/**
 * Stores `.crisp` bytes in the library. The file must open and be
 * structurally compliant; anything else is refused with the report.
 */
export async function addFile(bytes: Uint8Array): Promise<ImportResult> {
  const { bundle, summary, report } = await inspect(bytes);
  bundle.free();
  if (!report.compliant) {
    const failed = report.checks.find((c) => c.status === "fail");
    throw new Error(
      `This file does not verify${failed?.detail ? `: ${failed.detail}` : "."}`,
    );
  }
  const store = await getStore();
  const existing = await store.bundles.get(summary.doc_id);
  const now = Date.now();
  const record: BundleRecord = {
    docId: summary.doc_id,
    kind: summary.kind === "book" ? "book" : "recipe",
    title: summary.title,
    revision: summary.recipes[0]?.revision ?? "",
    bytes,
    report,
    published: report.trust === "endorsed" || (existing?.published ?? false),
    publishedTo: existing?.publishedTo,
    importedAt: existing?.importedAt ?? now,
    updatedAt: now,
  };
  await store.bundles.put(record);
  log.info("stored", {
    docId: record.docId,
    revision: record.revision,
    trust: report.trust,
    replaced: Boolean(existing),
  });
  return { entry: { record, summary, report }, replaced: Boolean(existing) };
}

/** Every stored file, newest first, opened and re-verified. */
export async function listEntries(): Promise<LibraryEntry[]> {
  const store = await getStore();
  const records = await store.bundles.all();
  const entries: LibraryEntry[] = [];
  for (const record of records) {
    try {
      const { bundle, summary, report } = await inspect(record.bytes);
      bundle.free();
      entries.push({ record, summary, report });
    } catch (error: unknown) {
      log.warn("unreadable", { docId: record.docId, error: String(error) });
    }
  }
  return entries;
}

/** Opens one stored file. The caller frees the returned bundle. */
export async function openEntry(
  docId: string,
): Promise<(LibraryEntry & { bundle: Bundle }) | null> {
  const store = await getStore();
  const record = await store.bundles.get(docId);
  if (!record) return null;
  const { bundle, summary, report } = await inspect(record.bytes);
  return { record, summary, report, bundle };
}

/** Removes a file, returning it so the removal can be undone. */
export async function removeEntry(docId: string): Promise<BundleRecord | null> {
  const store = await getStore();
  const record = (await store.bundles.get(docId)) ?? null;
  await store.bundles.delete(docId);
  log.info("removed", { docId });
  return record;
}

/** A server and account this device published a document as. */
export interface Publication {
  /** The server's base URL. */
  url: string;
  /** The account's username on that server. */
  username: string;
}

/** Narrows a stored `publishedTo` to the publications it lists. */
export function publications(value: unknown): Publication[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((p: unknown) =>
    isRecord(p) &&
    typeof p["url"] === "string" &&
    typeof p["username"] === "string"
      ? [{ url: p["url"], username: p["username"] }]
      : [],
  );
}

/**
 * Whether a record's stored `publishedTo` says this device published the
 * document to `url` as `username`.
 */
export function publishedHere(
  publishedTo: unknown,
  url: string,
  username: string,
): boolean {
  return publications(publishedTo).some(
    (p) => p.url === url && p.username === username,
  );
}

/** Records that this device published `docId` to `url` as `username`. */
export async function markPublished(
  docId: string,
  url: string,
  username: string,
): Promise<void> {
  const store = await getStore();
  const record = await store.bundles.get(docId);
  if (!record || publishedHere(record.publishedTo, url, username)) return;
  await store.bundles.put({
    ...record,
    published: true,
    publishedTo: [...publications(record.publishedTo), { url, username }],
    updatedAt: Date.now(),
  });
  log.info("published.recorded", { docId, url, username });
}

/**
 * Records that `url` no longer offers what `username` published of a
 * document. The document stays published while this device published it
 * elsewhere too. The local copy and its bytes stay exactly as they were.
 */
export async function markUnpublished(
  docId: string,
  url: string,
  username: string,
): Promise<void> {
  const store = await getStore();
  const record = await store.bundles.get(docId);
  if (!record) return;
  const remaining = publications(record.publishedTo).filter(
    (p) => p.url !== url || p.username !== username,
  );
  await store.bundles.put({
    ...record,
    published: remaining.length > 0,
    publishedTo: remaining,
    updatedAt: Date.now(),
  });
  log.info("unpublished", { docId, url, username });
}

/** Puts a removed file back. */
export async function restoreEntry(record: BundleRecord): Promise<void> {
  const store = await getStore();
  await store.bundles.put(record);
  log.info("restored", { docId: record.docId });
}

/** A safe file name for a recipe title. */
export function fileName(title: string): string {
  const slug = title
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80);
  return `${slug || "recipe"}${CRISP_EXTENSION}`;
}

/**
 * Hands a file to the user: the share sheet where the platform can share
 * files (phones), otherwise a download.
 */
export async function exportFile(
  bytes: Uint8Array,
  name: string,
  mediaType: string = CRISP_MEDIA_TYPE,
): Promise<"shared" | "downloaded" | "cancelled"> {
  const blob = new Blob([bytes.slice().buffer], { type: mediaType });
  const file = new File([blob], name, { type: mediaType });
  if (
    typeof navigator.canShare === "function" &&
    navigator.canShare({ files: [file] })
  ) {
    try {
      await navigator.share({ files: [file], title: name });
      return "shared";
    } catch (error: unknown) {
      if (error instanceof DOMException && error.name === "AbortError") {
        return "cancelled";
      }
      log.warn("share.failed", { error: String(error) });
    }
  }
  const url = URL.createObjectURL(blob);
  try {
    const link = document.createElement("a");
    link.href = url;
    link.download = name;
    document.body.append(link);
    link.click();
    link.remove();
  } finally {
    setTimeout(() => URL.revokeObjectURL(url), 10_000);
  }
  return "downloaded";
}

/** Reads a picked or dropped file's bytes. */
export async function readFile(file: Blob): Promise<Uint8Array> {
  return new Uint8Array(await file.arrayBuffer());
}
