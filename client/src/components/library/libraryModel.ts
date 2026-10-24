/**
 * Pure helpers behind the library screen: searching, sorting, restoring a
 * deleted record, and dates. No DOM, no engine — everything here
 * is unit-tested.
 */

import { isRecord } from "../../lib/isRecord";
import type { BundleRecord } from "../../lib/store";

/** The slice of a library entry the list logic needs. */
export interface ListableEntry {
  /** The stored record's bookkeeping. */
  record: { docId: string; title: string; updatedAt: number };
  /** The fresh summary (title in the reader's language). */
  summary: { title: string; authors: string[] };
}

/** How the library grid is ordered. */
export type LibrarySort = "recent" | "title";

/** The sort options, in menu order. */
export const SORT_OPTIONS: { value: LibrarySort; label: string }[] = [
  { value: "recent", label: "Recent" },
  { value: "title", label: "A–Z" },
];

/** Narrows a stored or user-picked value to a {@link LibrarySort}. */
export function isLibrarySort(value: unknown): value is LibrarySort {
  return value === "recent" || value === "title";
}

/** Lower-cases and strips accents so "creme" finds "Crème". */
export function foldText(text: string): string {
  return text
    .normalize("NFKD")
    .replace(/\p{Mn}/gu, "")
    .toLowerCase();
}

/** The display title of an entry, falling back to the stored title. */
export function entryTitle(entry: ListableEntry): string {
  return entry.summary.title.trim() || entry.record.title.trim() || "Untitled";
}

/**
 * Entries whose title or authors contain every word of `query`
 * (accent- and case-insensitive). An empty query keeps everything.
 */
export function filterEntries<T extends ListableEntry>(
  entries: readonly T[],
  query: string,
): T[] {
  const words = foldText(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...entries];
  return entries.filter((entry) => {
    const haystack = foldText(
      [entryTitle(entry), ...entry.summary.authors].join(" "),
    );
    return words.every((word) => haystack.includes(word));
  });
}

const collator = new Intl.Collator(undefined, {
  sensitivity: "base",
  numeric: true,
});

/** A sorted copy: most recently updated first, or by title A–Z. */
export function sortEntries<T extends ListableEntry>(
  entries: readonly T[],
  sort: LibrarySort,
): T[] {
  const copy = [...entries];
  if (sort === "title") {
    copy.sort(
      (a, b) =>
        collator.compare(entryTitle(a), entryTitle(b)) ||
        a.record.docId.localeCompare(b.record.docId),
    );
  } else {
    copy.sort(
      (a, b) =>
        b.record.updatedAt - a.record.updatedAt ||
        a.record.docId.localeCompare(b.record.docId),
    );
  }
  return copy;
}

/**
 * Narrows `location.state.removed` (set by the recipe page after a delete)
 * to a stored record that can be put back.
 */
export function removedFromState(state: unknown): BundleRecord | null {
  if (!isRecord(state)) return null;
  const r = state["removed"];
  if (!isRecord(r)) return null;
  const { docId, kind, title, revision, bytes, published } = r;
  const { importedAt, updatedAt } = r;
  if (
    typeof docId !== "string" ||
    !docId ||
    (kind !== "recipe" && kind !== "book") ||
    typeof title !== "string" ||
    typeof revision !== "string" ||
    !(bytes instanceof Uint8Array) ||
    typeof published !== "boolean" ||
    typeof importedAt !== "number" ||
    typeof updatedAt !== "number"
  ) {
    return null;
  }
  return {
    docId,
    kind,
    title,
    revision,
    bytes,
    report: r["report"],
    published,
    importedAt,
    updatedAt,
  };
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** "just now", "5 min ago", "3 h ago", "yesterday", or a short date. */
export function relativeTime(
  epochMs: number,
  now: number = Date.now(),
  locale?: string,
): string {
  const elapsed = Math.max(0, now - epochMs);
  if (elapsed < MINUTE) return "just now";
  if (elapsed < HOUR) return `${Math.floor(elapsed / MINUTE)} min ago`;
  if (elapsed < DAY) return `${Math.floor(elapsed / HOUR)} h ago`;
  if (elapsed < 2 * DAY) return "yesterday";
  if (elapsed < 7 * DAY) return `${Math.floor(elapsed / DAY)} days ago`;
  const date = new Date(epochMs);
  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  return date.toLocaleDateString(locale, {
    day: "numeric",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}
