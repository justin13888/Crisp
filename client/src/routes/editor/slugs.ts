/**
 * Free-form classification slugs (cuisine, tags): lowercase ASCII words
 * joined by single hyphens, at most 64 characters (`crisp-v1.cddl`: `slug`).
 */

/** The longest slug the format allows. */
export const MAX_SLUG = 64;

/** Normalizes typed text to a slug (`"Sichuan Food!"` → `sichuan-food`). */
export function normalizeSlug(text: string): string {
  return text
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, MAX_SLUG)
    .replace(/-+$/, "");
}

/** Splits comma- or newline-separated input into distinct slugs. */
export function parseSlugs(text: string): string[] {
  const out: string[] = [];
  for (const part of text.split(/[,\n;]/)) {
    const slug = normalizeSlug(part);
    if (slug && !out.includes(slug)) out.push(slug);
  }
  return out;
}

/** Adds slugs to a list, keeping order and skipping duplicates. */
export function addSlugs(list: readonly string[], text: string): string[] {
  const next = [...list];
  for (const slug of parseSlugs(text)) {
    if (!next.includes(slug)) next.push(slug);
  }
  return next;
}
