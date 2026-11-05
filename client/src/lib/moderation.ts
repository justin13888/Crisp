/**
 * Turning what a server advertises about its terms and its abuse contact
 * (`GET /server-info`'s `terms_url` and `abuse_contact`) into links. Both
 * come from the network, so only `http(s)` and `mailto:` addresses become
 * links; anything else is shown as text or not at all.
 */

/** Whether `value` is an absolute `http(s)` URL. */
function isWebUrl(value: string): boolean {
  return /^https?:\/\/[^\s/]+/i.test(value);
}

/** A link to the server's terms, or `null` when it published none to link. */
export function termsHref(termsUrl: string | undefined): string | null {
  const url = termsUrl?.trim();
  return url && isWebUrl(url) ? url : null;
}

/**
 * Where to report a document to the server's operator: the abuse contact
 * as a link, with `subject` (the caller's localised text naming the
 * document) as an e-mail's subject; or `null` when the contact is absent or
 * not an address a link can carry.
 */
export function abuseHref(
  contact: string | undefined,
  subject: string,
): string | null {
  const value = contact?.trim();
  if (!value) return null;
  if (isWebUrl(value)) return value;
  const query = `subject=${encodeURIComponent(subject)}`;
  if (/^mailto:\S+$/i.test(value)) {
    return `${value}${value.includes("?") ? "&" : "?"}${query}`;
  }
  if (/^[^\s@:/]+@[^\s@:/]+$/.test(value)) return `mailto:${value}?${query}`;
  return null;
}
