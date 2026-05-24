/**
 * Curated value lists for the two recipe fields that are not enumerated by
 * `crisp-core` itself: BCP-47 language tags and SPDX license identifiers.
 *
 * The format admits *any* well-formed BCP-47 tag and *any* SPDX id (plus the
 * sentinel `"NONE"`), but presenting the user with an open text field for
 * those is a footgun. The curated lists below are a tasteful subset; the
 * stored value is always the wire string (the `tag` or `id`), so changing a
 * label here is purely cosmetic.
 */

/** One entry in the curated language dropdown. */
export interface LanguageOption {
  /** The BCP-47 tag. */
  tag: string;
  /** The display name shown in the UI. */
  label: string;
}

/** One entry in the curated license dropdown. */
export interface LicenseOption {
  /** The SPDX identifier, or the sentinel `"NONE"`. */
  id: string;
  /** The display name shown in the UI. */
  label: string;
}

/** Common BCP-47 language tags. Every entry is within the 2-35 byte range. */
export const LANGUAGES: readonly LanguageOption[] = [
  { tag: "en", label: "English" },
  { tag: "en-US", label: "English (United States)" },
  { tag: "en-GB", label: "English (United Kingdom)" },
  { tag: "fr", label: "French" },
  { tag: "de", label: "German" },
  { tag: "es", label: "Spanish" },
  { tag: "it", label: "Italian" },
  { tag: "pt", label: "Portuguese" },
  { tag: "pt-BR", label: "Portuguese (Brazil)" },
  { tag: "nl", label: "Dutch" },
  { tag: "sv", label: "Swedish" },
  { tag: "pl", label: "Polish" },
  { tag: "ru", label: "Russian" },
  { tag: "uk", label: "Ukrainian" },
  { tag: "tr", label: "Turkish" },
  { tag: "ar", label: "Arabic" },
  { tag: "he", label: "Hebrew" },
  { tag: "hi", label: "Hindi" },
  { tag: "ja", label: "Japanese" },
  { tag: "ko", label: "Korean" },
  { tag: "zh-Hans", label: "Chinese (Simplified)" },
  { tag: "zh-Hant", label: "Chinese (Traditional)" },
  { tag: "th", label: "Thai" },
  { tag: "vi", label: "Vietnamese" },
];

/** Common licenses for recipes plus the format's `NONE` sentinel. */
export const LICENSES: readonly LicenseOption[] = [
  { id: "CC0-1.0", label: "CC0 1.0 (Public Domain)" },
  { id: "CC-BY-4.0", label: "CC BY 4.0" },
  { id: "CC-BY-SA-4.0", label: "CC BY-SA 4.0" },
  { id: "CC-BY-NC-4.0", label: "CC BY-NC 4.0" },
  { id: "CC-BY-ND-4.0", label: "CC BY-ND 4.0" },
  { id: "MIT", label: "MIT" },
  { id: "Apache-2.0", label: "Apache 2.0" },
  { id: "NONE", label: "No license (NONE)" },
];
