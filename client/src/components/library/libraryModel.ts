/**
 * Pure helpers behind the library screen: searching, sorting, trust labels,
 * draft links, and dates. No DOM, no engine — everything here is unit-tested.
 */

import type { Trust } from "../../lib/crisp";

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

/** How a trust level is shown: label, badge tone, and explanation. */
export interface TrustDisplay {
  /** Short badge text. */
  label: string;
  /** The badge class (`ok`, `warn`, or none). */
  tone: "ok" | "neutral" | "warn";
  /** A one-line explanation (badge tooltip / screen readers). */
  description: string;
}

/** The badge for a verification trust level. */
export function trustDisplay(trust: Trust): TrustDisplay {
  switch (trust) {
    case "endorsed":
      return {
        label: "Endorsed",
        tone: "ok",
        description: "Signed by its author and endorsed by a trusted server.",
      };
    case "author-signed":
      return {
        label: "Signed",
        tone: "neutral",
        description: "Signed by its author.",
      };
    default:
      return {
        label: "Unsigned",
        tone: "warn",
        description: "Not signed — its author cannot be verified.",
      };
  }
}

/** Where a stored editor draft is resumed, or `null` for unknown ids. */
export function draftLink(id: string): string | null {
  if (id === "new") return "/new";
  if (id.startsWith("edit:") && id.length > "edit:".length) {
    return `/edit/${encodeURIComponent(id.slice("edit:".length))}`;
  }
  return null;
}

/** A best-effort title for an opaque draft state. */
export function draftTitle(state: unknown, id: string): string {
  const fallback = id === "new" ? "New recipe" : "Edited recipe";
  if (typeof state !== "object" || state === null) return fallback;
  const record = state as Record<string, unknown>;
  for (const key of ["title", "name"]) {
    const value = record[key];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return fallback;
}

/** The stored-file shape (mirrors `BundleRecord` in `lib/store`). */
export interface RemovedRecord {
  /** The document id. */
  docId: string;
  /** Recipe or book. */
  kind: "recipe" | "book";
  /** The stored title. */
  title: string;
  /** The stored revision. */
  revision: string;
  /** The file bytes. */
  bytes: Uint8Array;
  /** The last report (untrusted). */
  report: unknown;
  /** Whether it was published. */
  published: boolean;
  /** When it entered the library (epoch ms). */
  importedAt: number;
  /** When it last changed (epoch ms). */
  updatedAt: number;
}

/**
 * Narrows `location.state.removed` (set by the recipe page after a delete)
 * to a stored record that can be put back.
 */
export function removedFromState(state: unknown): RemovedRecord | null {
  if (typeof state !== "object" || state === null) return null;
  const removed = (state as Record<string, unknown>)["removed"];
  if (typeof removed !== "object" || removed === null) return null;
  const r = removed as Record<string, unknown>;
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
