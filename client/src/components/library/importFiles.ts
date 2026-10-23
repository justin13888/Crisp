/**
 * The import pipeline shared by the library (drop / picker) and the import
 * screen (share target / file handler): every file is verified by the
 * engine and stored when compliant; failures carry the failed checks.
 */

import type { CheckResult, ComplianceReport, Trust } from "../../lib/crisp";
import { errorMessage } from "../../lib/crisp";
import { createLogger } from "../../lib/log";

const log = createLogger("import");

/** A file to import: a name for display and its untrusted bytes. */
export interface ImportSource {
  /** The original file name. */
  name: string;
  /** The raw bytes. */
  bytes: Uint8Array;
}

/** A failed verification check, as shown to the cook. */
export interface FailedCheck {
  /** The check id, e.g. `C3`. */
  id: string;
  /** The stable code, e.g. `E.ID.MISMATCH`. */
  code: string | undefined;
  /** What went wrong. */
  detail: string | undefined;
}

/** What happened to one imported file. */
export type ImportOutcome =
  | {
      /** Stored as a new recipe, or replaced an existing copy. */
      status: "added" | "updated";
      /** The file name. */
      name: string;
      /** The stored document id. */
      docId: string;
      /** The recipe title. */
      title: string;
      /** The verified trust level. */
      trust: Trust;
    }
  | {
      /** Refused. */
      status: "failed";
      /** The file name. */
      name: string;
      /** Why, in one sentence. */
      reason: string;
      /** The checks that failed, when the file could be opened. */
      failedChecks: FailedCheck[];
      /** The document title, when the file could be opened. */
      title: string | undefined;
    };

/** The library operations the pipeline needs (injected for tests). */
export interface ImportDeps {
  /** Verifies and stores; throws with a reason when refused. */
  add(bytes: Uint8Array): Promise<{
    entry: {
      record: { docId: string };
      summary: { title: string };
      report: { trust: Trust };
    };
    replaced: boolean;
  }>;
  /** Verifies without storing; throws when the file cannot be opened. */
  inspect(
    bytes: Uint8Array,
  ): Promise<{ title: string; report: ComplianceReport }>;
}

/** The checks of a report that failed. */
export function failedChecks(checks: readonly CheckResult[]): FailedCheck[] {
  return checks
    .filter((check) => check.status === "fail")
    .map((check) => ({ id: check.id, code: check.code, detail: check.detail }));
}

/** Imports one file. Never throws. */
export async function importOne(
  source: ImportSource,
  deps: ImportDeps,
): Promise<ImportOutcome> {
  const started = performance.now();
  log.debug("file.start", { name: source.name, size: source.bytes.length });
  try {
    const result = await deps.add(source.bytes);
    const outcome: ImportOutcome = {
      status: result.replaced ? "updated" : "added",
      name: source.name,
      docId: result.entry.record.docId,
      title: result.entry.summary.title,
      trust: result.entry.report.trust,
    };
    log.info("file.stored", {
      name: source.name,
      docId: outcome.docId,
      status: outcome.status,
      trust: outcome.trust,
      ms: Math.round(performance.now() - started),
    });
    return outcome;
  } catch (cause: unknown) {
    const reason = errorMessage(cause);
    // Re-verify only on failure, to show which checks failed.
    let checks: FailedCheck[] = [];
    let title: string | undefined;
    try {
      const inspected = await deps.inspect(source.bytes);
      checks = failedChecks(inspected.report.checks);
      title = inspected.title || undefined;
    } catch (inspectError: unknown) {
      log.debug("file.unopenable", {
        name: source.name,
        error: errorMessage(inspectError),
      });
    }
    log.warn("file.refused", {
      name: source.name,
      reason,
      codes: checks.map((c) => c.code ?? c.id),
    });
    return {
      status: "failed",
      name: source.name,
      reason,
      failedChecks: checks,
      title,
    };
  }
}

/** Imports files one after another (keeps memory and the UI calm). */
export async function importAll(
  sources: readonly ImportSource[],
  deps: ImportDeps,
  onProgress?: (outcome: ImportOutcome, index: number) => void,
): Promise<ImportOutcome[]> {
  const outcomes: ImportOutcome[] = [];
  for (const [index, source] of sources.entries()) {
    const outcome = await importOne(source, deps);
    outcomes.push(outcome);
    onProgress?.(outcome, index);
  }
  log.info("batch.done", { ...tallyOutcomes(outcomes) });
  return outcomes;
}

/** Counts per status. */
export interface ImportTally {
  /** New recipes. */
  added: number;
  /** Replaced copies. */
  updated: number;
  /** Refused files. */
  failed: number;
}

/** Counts outcomes by status. */
export function tallyOutcomes(outcomes: readonly ImportOutcome[]): ImportTally {
  const tally: ImportTally = { added: 0, updated: 0, failed: 0 };
  for (const outcome of outcomes) tally[outcome.status]++;
  return tally;
}

/** A one-line summary such as "2 added, 1 failed". */
export function describeTally(tally: ImportTally): string {
  const parts: string[] = [];
  if (tally.added) parts.push(`${tally.added} added`);
  if (tally.updated) parts.push(`${tally.updated} updated`);
  if (tally.failed) parts.push(`${tally.failed} failed`);
  return parts.length ? parts.join(", ") : "Nothing imported";
}
