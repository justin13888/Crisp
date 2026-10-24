/**
 * Free-form classification slugs (cuisine, tags): lowercase ASCII words
 * joined by single hyphens (`crisp-v1.cddl`: `slug`). The length bound is
 * the engine's (`recipeConstraints().limits.maxSlugBytes`), passed in as
 * `maxBytes`; a slug is ASCII, so bytes and characters coincide.
 */

/** Normalizes typed text to a slug (`"Sichuan Food!"` → `sichuan-food`). */
export function normalizeSlug(text: string, maxBytes: number): string {
  return text
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, maxBytes)
    .replace(/-+$/, "");
}

/** Splits comma- or newline-separated input into distinct slugs. */
export function parseSlugs(text: string, maxBytes: number): string[] {
  const out: string[] = [];
  for (const part of text.split(/[,\n;]/)) {
    const slug = normalizeSlug(part, maxBytes);
    if (slug && !out.includes(slug)) out.push(slug);
  }
  return out;
}

/** Adds slugs to a list, keeping order and skipping duplicates. */
export function addSlugs(
  list: readonly string[],
  text: string,
  maxBytes: number,
): string[] {
  const next = [...list];
  for (const slug of parseSlugs(text, maxBytes)) {
    if (!next.includes(slug)) next.push(slug);
  }
  return next;
}
