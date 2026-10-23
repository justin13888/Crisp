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
import { createLogger } from "./log";
import { trustedKeys } from "./servers";
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

/** Opens and verifies file bytes against every pinned server key. */
export async function inspect(
  bytes: Uint8Array,
): Promise<{
  bundle: Bundle;
  summary: BundleSummary;
  report: ComplianceReport;
}> {
  const engine = await loadEngine();
  const bundle = engine.Bundle.open(bytes);
  const report = bundle.verify(await trustedKeys(), nowSeconds());
  const summary = bundle.summary(preferredLanguages());
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
