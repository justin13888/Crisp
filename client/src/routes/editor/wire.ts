/**
 * Reading and writing the wire format's localized maps and prose, which
 * cross the engine boundary as plain objects keyed by language tag.
 */

import type {
  LocalizedProse,
  LocalizedText,
  Paragraph,
} from "../../generated/crisp";

/** Narrows an untrusted value (e.g. a restored draft) to a plain record. */
export function asRecord(value: unknown): Record<string, unknown> {
  if (typeof value === "object" && value !== null && !Array.isArray(value)) {
    return { ...(value as Record<string, unknown>) };
  }
  return {};
}

/** The text for `lang`, falling back to any language, else `""`. */
export function readText(
  value: LocalizedText | undefined,
  lang: string,
): string {
  const record = asRecord(value);
  const exact = record[lang];
  if (typeof exact === "string") return exact;
  for (const entry of Object.values(record)) {
    if (typeof entry === "string") return entry;
  }
  return "";
}

/**
 * Localized text with `lang` set to `text`, keeping the other languages of
 * `orig`. Returns `undefined` when `text` is blank (callers omit the field).
 */
export function writeText(
  lang: string,
  text: string,
  orig?: LocalizedText,
): LocalizedText | undefined {
  const value = text.trim();
  if (!value) return undefined;
  const record: Record<string, string> = {};
  for (const [key, entry] of Object.entries(asRecord(orig))) {
    if (typeof entry === "string") record[key] = entry;
  }
  record[lang] = value;
  return record;
}

/** Localized text that is always present (empty if blank; validation flags it). */
export function requiredText(
  lang: string,
  text: string,
  orig?: LocalizedText,
): LocalizedText {
  return writeText(lang, text, orig) ?? { [lang]: "" };
}

/** Splits typed text into paragraphs on blank lines. */
export function splitParagraphs(text: string): string[] {
  return text
    .replace(/\r\n?/g, "\n")
    .split(/\n[ \t]*\n/)
    .map((p) => p.trim())
    .filter((p) => p.length > 0);
}

/** Typed text as prose paragraphs, one plain run each. */
export function textToParagraphs(text: string): Paragraph[] {
  return splitParagraphs(text).map((t) => [{ t }]);
}

function isParagraphs(value: unknown): value is Paragraph[] {
  return (
    Array.isArray(value) &&
    value.every(
      (p) =>
        Array.isArray(p) &&
        p.every(
          (run) =>
            typeof run === "object" &&
            run !== null &&
            typeof (run as Record<string, unknown>)["t"] === "string",
        ),
    )
  );
}

/** Prose paragraphs as editable text (runs joined, paragraphs blank-separated). */
export function paragraphsToText(paragraphs: Paragraph[]): string {
  return paragraphs.map((p) => p.map((run) => run.t).join("")).join("\n\n");
}

/** The prose for `lang` as editable text, falling back to any language. */
export function readProse(
  value: LocalizedProse | undefined,
  lang: string,
): string {
  const record = asRecord(value);
  const exact = record[lang];
  if (isParagraphs(exact)) return paragraphsToText(exact);
  for (const entry of Object.values(record)) {
    if (isParagraphs(entry)) return paragraphsToText(entry);
  }
  return "";
}

/**
 * Localized prose with `lang` set from typed `text`, keeping the other
 * languages of `orig`. When the text is unchanged from `orig`, `orig` is
 * returned as-is so rich runs (styles, links, refs) survive an edit.
 * Returns `undefined` when `text` is blank.
 */
export function writeProse(
  lang: string,
  text: string,
  orig?: LocalizedProse,
): LocalizedProse | undefined {
  const paragraphs = textToParagraphs(text);
  if (paragraphs.length === 0) return undefined;
  const record = asRecord(orig);
  const previous = record[lang];
  if (
    orig &&
    isParagraphs(previous) &&
    paragraphsToText(previous) === paragraphsToText(paragraphs)
  ) {
    return orig;
  }
  const next: Record<string, Paragraph[]> = {};
  for (const [key, entry] of Object.entries(record)) {
    if (isParagraphs(entry)) next[key] = entry;
  }
  next[lang] = paragraphs;
  return next;
}

/** Localized prose that is always present (one empty run if blank). */
export function requiredProse(
  lang: string,
  text: string,
  orig?: LocalizedProse,
): LocalizedProse {
  return writeProse(lang, text, orig) ?? { [lang]: [[{ t: "" }]] };
}
