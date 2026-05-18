/**
 * TypeScript projections of the Crisp data model.
 *
 * These mirror `spec/crisp-v1.cddl` and the `crisp-core` Rust types. The WASM
 * boundary hands back plain objects typed as `unknown`; the guards in
 * `lib/crisp.ts` narrow them to these shapes before the UI ever reads them.
 */

/** A BCP-47 language tag, e.g. `"en-US"`. */
export type LanguageTag = string;

/** A localized single-line string: language tag → text. */
export type LocalizedText = Record<LanguageTag, string>;

/** A localized rich-text block: language tag → prose. */
export type LocalizedProse = Record<LanguageTag, Prose>;

/** Rich text: an ordered list of paragraphs. */
export type Prose = Paragraph[];

/** A paragraph: an ordered list of styled runs. */
export type Paragraph = Run[];

/** A character-style flag. */
export type StyleFlag = "bold" | "italic" | "underline" | "code";

/** A run of uniformly styled text. */
export interface Run {
  /** The literal text. */
  t: string;
  /** Optional style flags. */
  style?: StyleFlag[];
  /** An optional `https://` hyperlink. */
  link?: string;
}

/** An exact rational number, encoded as `[numerator, denominator]`. */
export type Rational = [number, number];

/** An amount, possibly a range, possibly non-numeric. */
export interface Quantity {
  /** The amount or lower bound. */
  min?: Rational;
  /** The upper bound of a range. */
  max?: Rational;
  /** The unit code. */
  unit?: string;
  /** Whether the amount is approximate. */
  approx?: boolean;
  /** A non-numeric amount, e.g. "to taste". */
  text?: LocalizedText;
}

/** An author's composite public key set. */
export interface PublicKeySet {
  /** The Ed25519 public key. */
  ed25519: Uint8Array;
  /** The ML-DSA-65 public key. */
  "ml-dsa-65": Uint8Array;
}

/** An author of a recipe or book. */
export interface Author {
  /** The author's `usr…` id. */
  uid: string;
  /** A display name. */
  display_name: string;
  /** The author's public key set. */
  keyset: PublicKeySet;
}

/** A recipe yield. */
export interface Yield {
  /** The amount produced. */
  amount: Quantity;
  /** The localized unit, e.g. "servings". */
  unit_text: LocalizedText;
}

/** A single ingredient. */
export interface Ingredient {
  /** A stable within-recipe handle. */
  ref: number;
  /** The ingredient-group index, if grouped. */
  group?: number;
  /** The ingredient name. */
  name: LocalizedText;
  /** The quantity called for. */
  quantity?: Quantity;
  /** A human-authored amount override. */
  display?: LocalizedText;
  /** A preparation note. */
  preparation?: LocalizedText;
  /** Whether the ingredient is optional. */
  optional?: boolean;
  /** Acceptable substitutes. */
  substitutes?: { name: LocalizedText; quantity?: Quantity }[];
}

/** A piece of equipment. */
export interface Equipment {
  /** A stable within-recipe handle. */
  ref: number;
  /** The equipment name. */
  name: LocalizedText;
  /** An optional note. */
  note?: LocalizedText;
}

/** A single recipe step. */
export interface Step {
  /** A stable within-recipe handle. */
  ref: number;
  /** An optional step-group name. */
  group?: LocalizedText;
  /** The instruction text. */
  text: LocalizedProse;
  /** Handles of ingredients used in this step. */
  ingredient_refs?: number[];
  /** Handles of equipment used in this step. */
  equipment_refs?: number[];
  /** An optional duration driving a client timer. */
  duration?: Quantity;
  /** An optional target temperature. */
  temperature?: { value: Rational; unit: string };
}

/** The frozen birth identity of a recipe. */
export interface Identity {
  /** `SHA-256` of the author public key set. */
  author_fpr: Uint8Array;
  /** The original normalized title. */
  title: string;
  /** The title language. */
  language: LanguageTag;
  /** The creation instant (epoch seconds). */
  created_at: number;
  /** A 128-bit birth nonce. */
  birth_nonce: Uint8Array;
}

/** A complete recipe. */
export interface Recipe {
  /** The frozen birth identity. */
  identity: Identity;
  /** The recipe id, derived from `identity`. */
  id: string;
  /** The revision hash. */
  revision: string;
  /** When last modified (epoch seconds). */
  modified_at: number;
  /** The displayed title. */
  title: LocalizedText;
  /** An optional summary. */
  summary?: LocalizedProse;
  /** What the recipe yields. */
  yield: Yield;
  /** Named durations keyed by phase. */
  times?: Record<string, Quantity>;
  /** Ingredient group names. */
  ingredient_groups?: LocalizedText[];
  /** The ingredients. */
  ingredients: Ingredient[];
  /** The equipment. */
  equipment?: Equipment[];
  /** The ordered steps. */
  steps: Step[];
  /** Keyword tags. */
  tags?: string[];
  /** Cuisine tags. */
  cuisine?: string[];
  /** Course classifications. */
  course?: string[];
  /** Diet classifications. */
  diet?: string[];
  /** Declared allergens. */
  allergens?: string[];
  /** Free-form notes. */
  notes?: LocalizedProse;
  /** Attribution text. */
  attribution?: LocalizedProse;
}

/** Document-level metadata. */
export interface DocumentMeta {
  /** The document title. */
  title: LocalizedText;
  /** The authors. */
  authors: Author[];
  /** An SPDX license id, or "NONE". */
  license: string;
  /** When created (epoch seconds). */
  created_at: number;
  /** When last modified (epoch seconds). */
  modified_at: number;
  /** An optional description. */
  description?: LocalizedProse;
}

/** Whether a bundle holds a recipe or a book. */
export type Kind = "recipe" | "book";

/** The signed contents of a bundle. */
export interface BundlePayload {
  /** The format version. */
  v: number;
  /** The minimum reader version. */
  min_v: number;
  /** Whether this is a recipe or a book. */
  kind: Kind;
  /** The document id. */
  doc_id: string;
  /** The default language. */
  default_language: LanguageTag;
  /** Every language used in the bundle. */
  languages: LanguageTag[];
  /** The document metadata. */
  meta: DocumentMeta;
  /** The recipes (at least one). */
  recipes: Recipe[];
}

/** The status of one verification check. */
export type CheckStatus = "pass" | "fail" | "skipped";

/** The result of one verification check. */
export interface CheckResult {
  /** The check id, e.g. "A1" or "C3". */
  id: string;
  /** Whether it passed, failed, or was skipped. */
  status: CheckStatus;
  /** The stable error code, on failure. */
  code?: string;
  /** A human-readable explanation, on failure. */
  detail?: string;
}

/** The compliance class a bundle achieved. */
export type ComplianceClass = "structural" | "published";

/** How much trust a verified bundle carries. */
export type Trust = "unsigned" | "author-signed" | "endorsed";

/** A complete verification verdict. */
export interface ComplianceReport {
  /** Always `1`. */
  crisp_report: number;
  /** Whether the bundle is compliant for its class. */
  compliant: boolean;
  /** The class the report is about. */
  class: ComplianceClass;
  /** The declared format version. */
  format_version: number;
  /** The trust level established. */
  trust: Trust;
  /** When verification ran (epoch seconds). */
  verified_at: number;
  /** Every check, in order. */
  checks: CheckResult[];
}
