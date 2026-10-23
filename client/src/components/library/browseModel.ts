/**
 * Pure helpers behind the browse screen: facet selection and the query sent
 * to the server.
 */

import type { SearchQuery } from "../../lib/crisp";

/** Results fetched per page. */
export const PAGE_SIZE = 20;

/** A facet group the cook can filter by. */
export type FacetKind = "course" | "diet" | "allergen_free";

/** What the cook has asked for. */
export interface BrowseFilters {
  /** Free text. */
  q: string;
  /** Selected courses. */
  course: string[];
  /** Selected diets. */
  diet: string[];
  /** Allergens the recipe must be free of. */
  allergen_free: string[];
}

/** No filters at all. */
export const EMPTY_FILTERS: BrowseFilters = {
  q: "",
  course: [],
  diet: [],
  allergen_free: [],
};

/** Adds `value` to a facet, or removes it when already selected. */
export function toggleFacet(
  filters: BrowseFilters,
  kind: FacetKind,
  value: string,
): BrowseFilters {
  const current = filters[kind];
  const next = current.includes(value)
    ? current.filter((v) => v !== value)
    : [...current, value];
  return { ...filters, [kind]: next };
}

/** Whether anything narrows the listing. */
export function hasFilters(filters: BrowseFilters): boolean {
  return (
    filters.q.trim() !== "" ||
    filters.course.length > 0 ||
    filters.diet.length > 0 ||
    filters.allergen_free.length > 0
  );
}

/** How many facet chips are selected. */
export function facetCount(filters: BrowseFilters): number {
  return (
    filters.course.length + filters.diet.length + filters.allergen_free.length
  );
}

/** One page request: plain browsing or a search. */
export type PageRequest =
  | { kind: "browse"; limit: number; offset: number }
  | { kind: "search"; query: SearchQuery };

/**
 * The server request for a page: plain browsing when nothing is filtered,
 * otherwise a search carrying only the non-empty fields.
 */
export function pageRequest(
  filters: BrowseFilters,
  offset: number,
  limit: number = PAGE_SIZE,
): PageRequest {
  if (!hasFilters(filters)) return { kind: "browse", limit, offset };
  const query: SearchQuery = { limit, offset };
  const q = filters.q.trim();
  if (q) query.q = q;
  if (filters.course.length) query.course = [...filters.course];
  if (filters.diet.length) query.diet = [...filters.diet];
  if (filters.allergen_free.length) {
    query.allergen_free = [...filters.allergen_free];
  }
  return { kind: "search", query };
}

/** Whether another page may exist after receiving `received` results. */
export function mayHaveMore(
  received: number,
  limit: number = PAGE_SIZE,
): boolean {
  return received >= limit;
}

/** "tree-nuts" → "Tree nuts", "gluten-free" → "Gluten free". */
export function facetLabel(code: string): string {
  const words = code.replace(/[-_]+/g, " ").trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}
