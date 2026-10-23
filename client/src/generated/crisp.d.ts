/* tslint:disable */
/* eslint-disable */

/** An exact rational number: `[numerator, denominator]`, denominator >= 1. */
export type Rational = [number, number];

/** A failure thrown by any crisp-wasm function. */
export interface CrispFailure extends Error {
    /** A stable verification code such as `E.REF`, when there is one. */
    code?: string;
    /** The HTTP status, for server failures. */
    status?: number;
    /** Every located problem, for draft validation failures. */
    issues?: Issue[];
}


/**
 * A BCP-47 language tag, e.g. `"en-US"` (`crisp-v1.cddl`: `language-tag`).
 */
export type LanguageTag = string;

/**
 * A UCUM temperature unit (`crisp-v1.cddl`: `temperature.unit`).
 */
export type TemperatureUnit = "Cel" | "[degF]" | "K";

/**
 * A UCUM unit code, e.g. `"g"` or `"cup"` (`crisp-v1.cddl`: `unit-code`).
 */
export type UnitCode = string;

/**
 * A blob's media type and bytes.
 */
export interface BlobData {
    /**
     * The media type, e.g. `image/webp`.
     */
    media_type: string;
    /**
     * The image bytes.
     */
    bytes: Uint8Array;
}

/**
 * A book identifier (`rbk…`).
 */
export type BookId = string;

/**
 * A chapter entry — either a recipe reference or an inline page
 * (`crisp-v1.cddl`: `entry`).
 */
export type Entry = { type: "recipe"; recipe: number } | { type: "page"; page: Page };

/**
 * A chapter of a book (`crisp-v1.cddl`: `chapter`).
 */
export interface Chapter {
    /**
     * The chapter title.
     */
    title: LocalizedText;
    /**
     * An optional chapter introduction.
     */
    intro?: LocalizedProse;
    /**
     * The chapter's entries; at least one.
     */
    entries: Entry[];
    /**
     * Namespaced extension data.
     */
    ext?: Record<string, unknown>;
}

/**
 * A character-style flag for a [`Run`] (`crisp-v1.cddl`: `style-flag`).
 */
export type StyleFlag = "bold" | "italic" | "underline" | "code";

/**
 * A complete recipe (`crisp-v1.cddl`: `recipe`).
 *
 * `Eq` is not derived because the optional `ext` map holds arbitrary CBOR,
 * which is only `PartialEq`.
 */
export interface Recipe {
    /**
     * The frozen birth identity.
     */
    identity: Identity;
    /**
     * The recipe id, derived from `identity`.
     */
    id: RecipeId;
    /**
     * The revision hash, derived from this recipe's content.
     */
    revision: RevisionId;
    /**
     * The revision this one supersedes, forming a hash-linked history.
     */
    supersedes?: RevisionId;
    /**
     * The parent recipe this one was forked from.
     */
    derived_from?: RecipeId;
    /**
     * When the recipe was last modified.
     */
    modified_at: Timestamp;
    /**
     * The displayed title.
     */
    title: LocalizedText;
    /**
     * An optional short summary.
     */
    summary?: LocalizedProse;
    /**
     * What the recipe yields.
     */
    yield: Yield;
    /**
     * Whole-recipe durations.
     */
    times?: Times;
    /**
     * Names of the ingredient groups, referenced by `ingredient.group`.
     */
    ingredient_groups?: LocalizedText[];
    /**
     * The ingredients.
     */
    ingredients: Ingredient[];
    /**
     * The equipment used.
     */
    equipment?: Equipment[];
    /**
     * Names of the step groups, referenced by `step.group`.
     */
    step_groups?: LocalizedText[];
    /**
     * The ordered steps; at least one.
     */
    steps: Step[];
    /**
     * Optional nutrition information.
     */
    nutrition?: Nutrition;
    /**
     * Language-neutral keyword tags (slugs).
     */
    tags?: string[];
    /**
     * Language-neutral cuisine tags (slugs).
     */
    cuisine?: string[];
    /**
     * Course classifications.
     */
    course?: string[];
    /**
     * Diet classifications.
     */
    diet?: string[];
    /**
     * Declared allergens.
     */
    allergens?: string[];
    /**
     * Associated media blobs; the first is the recipe's cover image.
     */
    media?: BlobId[];
    /**
     * Free-form notes.
     */
    notes?: LocalizedProse;
    /**
     * Where the recipe came from.
     */
    source?: Source;
    /**
     * Attribution text.
     */
    attribution?: LocalizedProse;
    /**
     * Namespaced extension data, ignored by conformant readers.
     */
    ext?: Record<string, unknown>;
}

/**
 * A complete verification verdict (`crisp-v1.cddl`: `compliance-report`).
 */
export interface ComplianceReport {
    /**
     * Always [`CRISP_REPORT_MARKER`].
     */
    crisp_report: number;
    /**
     * Whether the bundle is compliant for its [`ComplianceClass`].
     */
    compliant: boolean;
    /**
     * The class the report is about.
     */
    class: ComplianceClass;
    /**
     * The bundle's declared format version (`0` if it could not be read).
     */
    format_version: number;
    /**
     * The trust level established.
     */
    trust: Trust;
    /**
     * When verification was performed.
     */
    verified_at: Timestamp;
    /**
     * Every check, in order.
     */
    checks: CheckResult[];
}

/**
 * A content-addressed blob identifier (`blb…`).
 */
export type BlobId = string;

/**
 * A course classification (`crisp-v1.cddl`: `course-code`).
 */
export type CourseCode = "appetizer" | "main" | "side" | "dessert" | "beverage" | "breakfast" | "sauce" | "baked" | "snack";

/**
 * A declared allergen (`crisp-v1.cddl`: `allergen-code`).
 */
export type AllergenCode = "gluten" | "crustaceans" | "eggs" | "fish" | "peanuts" | "soybeans" | "milk" | "tree-nuts" | "celery" | "mustard" | "sesame" | "sulphites" | "lupin" | "molluscs";

/**
 * A diet classification (`crisp-v1.cddl`: `diet-code`).
 */
export type DietCode = "vegan" | "vegetarian" | "pescatarian" | "gluten-free" | "dairy-free" | "nut-free" | "halal" | "kosher" | "keto" | "low-fodmap";

/**
 * A document identifier — a bundle's `doc_id`, which is a recipe id for a
 * recipe bundle and a book id for a book bundle.
 */
export type DocId = string;

/**
 * A labelled formatted value (times, nutrition).
 */
export interface LabeledValue {
    /**
     * The label, e.g. `"Prep"`.
     */
    label: string;
    /**
     * The value, e.g. `"15 min"`.
     */
    value: string;
}

/**
 * A login challenge to answer with `Signer.signLogin`.
 */
export interface Challenge {
    /**
     * The hex nonce.
     */
    nonce: string;
    /**
     * The platform `usr…` id the answer is bound to.
     */
    audience: string;
}

/**
 * A non-recipe content page (`crisp-v1.cddl`: `page`).
 */
export interface Page {
    /**
     * The page title.
     */
    title: LocalizedText;
    /**
     * The page body.
     */
    body: LocalizedProse;
    /**
     * An optional page image.
     */
    blob?: BlobId;
    /**
     * Namespaced extension data.
     */
    ext?: Record<string, unknown>;
}

/**
 * A nutrient code (`crisp-v1.cddl`: `nutrient-code`).
 */
export type NutrientCode = "energy_kcal" | "protein_g" | "fat_g" | "saturated_fat_g" | "carb_g" | "sugar_g" | "fiber_g" | "sodium_mg";

/**
 * A page of published recipes.
 */
export interface RemotePage {
    /**
     * The recipes.
     */
    recipes: RemoteRecipe[];
    /**
     * The page size asked for.
     */
    limit: number;
    /**
     * The offset asked for.
     */
    offset: number;
}

/**
 * A paragraph: an ordered list of styled runs (`crisp-v1.cddl`: `paragraph`).
 */
export type Paragraph = Run[];

/**
 * A part of a book (`crisp-v1.cddl`: `part`).
 */
export interface Part {
    /**
     * The part title.
     */
    title: LocalizedText;
    /**
     * The chapters in this part; at least one.
     */
    chapters: Chapter[];
    /**
     * Namespaced extension data.
     */
    ext?: Record<string, unknown>;
}

/**
 * A piece of equipment (`crisp-v1.cddl`: `equipment`).
 */
export interface Equipment {
    /**
     * A stable within-recipe handle, referenced by `step.equipment`.
     */
    ref: number;
    /**
     * The equipment name.
     */
    name: LocalizedText;
    /**
     * How many are needed.
     */
    quantity?: Quantity;
    /**
     * An optional note, e.g. a size.
     */
    note?: LocalizedText;
    /**
     * An equipment photo.
     */
    blob?: BlobId;
    /**
     * Namespaced extension data.
     */
    ext?: Record<string, unknown>;
}

/**
 * A recipe identifier (`rcp…`).
 */
export type RecipeId = string;

/**
 * A revision hash (`rev…`) naming one exact version of a recipe.
 */
export type RevisionId = string;

/**
 * A rich-text block in one or more languages (`crisp-v1.cddl`:
 * `localized-prose`). Must be non-empty.
 */
export type LocalizedProse = Map<LanguageTag, Prose>;

/**
 * A rich-text body: an ordered list of paragraphs (`crisp-v1.cddl`: `prose`).
 */
export type Prose = Paragraph[];

/**
 * A run of styled text.
 */
export interface RunView {
    /**
     * The text.
     */
    text: string;
    /**
     * Bold.
     */
    bold?: boolean;
    /**
     * Italic.
     */
    italic?: boolean;
    /**
     * Underlined.
     */
    underline?: boolean;
    /**
     * Monospace.
     */
    code?: boolean;
    /**
     * An `https` link (already verified).
     */
    link?: string;
    /**
     * An internal reference: `"ingredient:3"`, `"step:2"`,
     * `"equipment:1"`, or `"recipe:rcp1…"`.
     */
    reference?: string;
}

/**
 * A run of text with uniform styling (`crisp-v1.cddl`: `run`).
 */
export interface Run {
    /**
     * The literal text of the run (NFC-normalized).
     */
    t: string;
    /**
     * Optional style flags applied to the whole run.
     */
    style?: StyleFlag[];
    /**
     * An optional outbound hyperlink; always an `https://` URL.
     */
    link?: string;
    /**
     * An optional typed internal reference.
     */
    ref?: RunRef;
}

/**
 * A search over published recipes. Lists are matched as "any of".
 */
export interface SearchQuery {
    /**
     * Free text.
     */
    q?: string | undefined;
    /**
     * Cuisine slugs.
     */
    cuisine?: string[];
    /**
     * Course codes.
     */
    course?: string[];
    /**
     * Diet codes.
     */
    diet?: string[];
    /**
     * Allergens the recipe must not declare.
     */
    allergen_free?: string[];
    /**
     * An identity language.
     */
    language?: string | undefined;
    /**
     * The page size (1–200).
     */
    limit?: number | undefined;
    /**
     * The offset.
     */
    offset?: number | undefined;
}

/**
 * A single ingredient (`crisp-v1.cddl`: `ingredient`).
 */
export interface Ingredient {
    /**
     * A stable within-recipe handle, referenced by steps.
     */
    ref: number;
    /**
     * The index of the ingredient group this belongs to, if any.
     */
    group?: number;
    /**
     * The ingredient name.
     */
    name: LocalizedText;
    /**
     * The quantity called for.
     */
    quantity?: Quantity;
    /**
     * Equivalent measures of the same amount, e.g. grams for a cup.
     */
    alt?: Quantity[];
    /**
     * When `true`, the amount never scales with the batch size.
     */
    fixed?: boolean;
    /**
     * Preparation note, e.g. "finely chopped".
     */
    preparation?: LocalizedText;
    /**
     * A short note, e.g. "room temperature".
     */
    note?: LocalizedText;
    /**
     * Whether the ingredient is optional.
     */
    optional?: boolean;
    /**
     * Acceptable substitutes.
     */
    substitutes?: Substitute[];
    /**
     * A component recipe this ingredient is made by (not resolved).
     */
    recipe?: RecipeId;
    /**
     * An ingredient photo.
     */
    blob?: BlobId;
    /**
     * Namespaced extension data.
     */
    ext?: Record<string, unknown>;
}

/**
 * A single recipe step (`crisp-v1.cddl`: `step`).
 */
export interface Step {
    /**
     * A stable within-recipe handle.
     */
    ref: number;
    /**
     * The index of the step group this belongs to, if any.
     */
    group?: number;
    /**
     * The instruction text.
     */
    text: LocalizedProse;
    /**
     * The ingredients used in this step.
     */
    ingredients?: StepIngredient[];
    /**
     * Handles of the equipment used in this step.
     */
    equipment?: number[];
    /**
     * Timers a cook can start during this step.
     */
    timers?: Timer[];
    /**
     * A stovetop heat level.
     */
    heat?: HeatLevel;
    /**
     * A target temperature.
     */
    temperature?: Temperature;
    /**
     * Names an intermediate result produced by this step.
     */
    produces?: LocalizedText;
    /**
     * Tips and warnings for this step.
     */
    note?: LocalizedProse;
    /**
     * A step photo.
     */
    blob?: BlobId;
    /**
     * Namespaced extension data.
     */
    ext?: Record<string, unknown>;
}

/**
 * A single-line string in one or more languages (`crisp-v1.cddl`:
 * `localized-text`). Must be non-empty.
 */
export type LocalizedText = Map<LanguageTag, string>;

/**
 * A stovetop heat level (`crisp-v1.cddl`: `heat-level`).
 */
export type HeatLevel = "low" | "medium-low" | "medium" | "medium-high" | "high";

/**
 * A substitute for an ingredient (`crisp-v1.cddl`: `substitute`).
 */
export interface Substitute {
    /**
     * The substitute's name.
     */
    name: LocalizedText;
    /**
     * The substitute quantity, if it differs.
     */
    quantity?: Quantity;
    /**
     * Namespaced extension data.
     */
    ext?: Record<string, unknown>;
}

/**
 * A target temperature for a step (`crisp-v1.cddl`: `temperature`).
 */
export interface Temperature {
    /**
     * The temperature, or the lower bound of a range.
     */
    min: Rational;
    /**
     * The upper bound of a range.
     */
    max?: Rational;
    /**
     * The temperature unit.
     */
    unit: TemperatureUnit;
    /**
     * What the temperature applies to.
     */
    target?: TemperatureTarget;
}

/**
 * A timer a cook can start.
 */
export interface TimerView {
    /**
     * What the timer is for.
     */
    label: string;
    /**
     * The duration in seconds (the lower bound of a range).
     */
    seconds: number;
    /**
     * The upper bound of a range, in seconds.
     */
    max_seconds: number | undefined;
    /**
     * The duration, formatted.
     */
    text: string;
    /**
     * Whether the wait is hands-off.
     */
    passive: boolean;
}

/**
 * A timer attached to a step (`crisp-v1.cddl`: `timer`).
 */
export interface Timer {
    /**
     * The duration, in a UCUM time unit; a range runs to `max`.
     */
    duration: Quantity;
    /**
     * What the timer is for, e.g. "simmer".
     */
    label?: LocalizedText;
    /**
     * Whether the wait is hands-off.
     */
    passive?: boolean;
    /**
     * Namespaced extension data.
     */
    ext?: Record<string, unknown>;
}

/**
 * A titled run of ingredients.
 */
export interface IngredientSection {
    /**
     * The group title, if the ingredients are grouped.
     */
    title: string | undefined;
    /**
     * The ingredients, in recipe order.
     */
    items: IngredientView[];
}

/**
 * A trusted platform endorsement key and the window it may sign in
 * (spec §8.5).
 */
export interface PlatformKey {
    /**
     * The platform's composite public key.
     */
    keyset: PublicKeySet;
    /**
     * The first instant the key may sign.
     */
    valid_from: Timestamp;
    /**
     * The last instant the key may sign, if it has been retired.
     */
    valid_until?: Timestamp;
}

/**
 * A typed pointer from a run to another element (`crisp-v1.cddl`: `run-ref`).
 */
export interface RunRef {
    /**
     * What kind of element the reference points at.
     */
    kind: RunRefKind;
    /**
     * The target — a within-recipe handle, or a recipe id.
     */
    target: RunTarget;
}

/**
 * A user identifier (`usr…`).
 */
export type UserId = string;

/**
 * A whole recipe, ready to lay out.
 */
export interface RecipeView {
    /**
     * The recipe id.
     */
    id: string;
    /**
     * The revision id.
     */
    revision: string;
    /**
     * The language the view was resolved in.
     */
    language: string;
    /**
     * The title.
     */
    title: string;
    /**
     * The summary.
     */
    summary: ParagraphView[];
    /**
     * The yield, formatted, e.g. `"24 cookies"`.
     */
    yield_text: string;
    /**
     * The numeric yield after scaling, when there is one.
     */
    yield_amount: Rational | null;
    /**
     * The yield's unit text, e.g. `"cookies"`.
     */
    yield_unit: string;
    /**
     * Whole-recipe times.
     */
    times: LabeledValue[];
    /**
     * Ingredients, grouped.
     */
    ingredients: IngredientSection[];
    /**
     * Equipment.
     */
    equipment: EquipmentView[];
    /**
     * Steps.
     */
    steps: StepView[];
    /**
     * Total active timer time, in seconds.
     */
    active_timer_seconds: number;
    /**
     * Total passive (hands-off) timer time, in seconds.
     */
    passive_timer_seconds: number;
    /**
     * Notes.
     */
    notes: ParagraphView[];
    /**
     * Attribution.
     */
    attribution: ParagraphView[];
    /**
     * Source.
     */
    source: SourceView | undefined;
    /**
     * Tags, cuisine, course, and diet codes.
     */
    tags: string[];
    /**
     * Declared allergens.
     */
    allergens: string[];
    /**
     * Nutrition per basis.
     */
    nutrition: LabeledValue[];
    /**
     * The nutrition basis, formatted.
     */
    nutrition_basis: string | undefined;
    /**
     * Image blob ids; the first is the cover.
     */
    media: string[];
    /**
     * The author display names.
     */
    authors: string[];
    /**
     * The license.
     */
    license: string;
    /**
     * The recipe id this was forked from.
     */
    derived_from: string | undefined;
    /**
     * When last modified (epoch seconds).
     */
    modified_at: number;
}

/**
 * An amount, possibly a range, possibly non-numeric (`crisp-v1.cddl`:
 * `quantity`).
 *
 * At least one of `min` or `text` must be present; that rule is a Phase D
 * verification check ([`crate::ErrorCode::Quantity`]), not a constructor
 * invariant.
 */
export interface Quantity {
    /**
     * The amount, or the lower bound of a range.
     */
    min?: Rational;
    /**
     * The upper bound of a range, when the quantity spans one.
     */
    max?: Rational;
    /**
     * The unit the amount is measured in.
     */
    unit?: UnitCode;
    /**
     * Whether the amount is approximate.
     */
    approx?: boolean;
    /**
     * A non-numeric amount, e.g. "to taste".
     */
    text?: LocalizedText;
}

/**
 * An authenticated session.
 */
export interface Session {
    /**
     * The account's `usr…` id.
     */
    user_id: string;
    /**
     * The bearer token.
     */
    token: string;
    /**
     * When the token expires (epoch seconds).
     */
    expires_at: number;
}

/**
 * An author of a recipe or book (`crisp-v1.cddl`: `author`).
 */
export interface Author {
    /**
     * The author's user id (`usr…`), derived from `keyset`.
     */
    uid: UserId;
    /**
     * A human-readable display name.
     */
    display_name: string;
    /**
     * The author's composite public key set.
     */
    keyset: PublicKeySet;
}

/**
 * An author's composite public key (`crisp-v1.cddl`: `public-key-set`).
 *
 * Its fingerprint ([`crate::identity::key_fingerprint`]) appears as a
 * recipe's `identity.author_fpr`, as a signature's `kid`, and inside the
 * author's `usr…` id.
 */
export interface PublicKeySet {
    /**
     * The Ed25519 public key (32 bytes).
     */
    ed25519: Uint8Array;
    /**
     * The ML-DSA-65 public key (1952 bytes).
     */
    "ml-dsa-65": Uint8Array;
}

/**
 * An image ready to add to a draft's `blobs`.
 */
export interface ImageBlob {
    /**
     * The content-addressed blob id.
     */
    id: string;
    /**
     * The blob.
     */
    blob: Blob;
}

/**
 * An ingredient as used by a step.
 */
export interface StepIngredientView {
    /**
     * The ingredient's `ref` handle.
     */
    ref: number;
    /**
     * The name.
     */
    name: string;
    /**
     * The amount used in this step (the portion, or the whole amount).
     */
    amount: string;
    /**
     * Preparation, e.g. "diced".
     */
    preparation: string | undefined;
}

/**
 * An ingredient used by a step (`crisp-v1.cddl`: `step-ingredient`).
 */
export interface StepIngredient {
    /**
     * The ingredient's `ref` handle.
     */
    ref: number;
    /**
     * The portion used in this step, when only part of it is.
     */
    quantity?: Quantity;
}

/**
 * An inline image. Every blob is content-addressed: its `blb…` id is the
 * bech32m encoding of `SHA-256(bytes)`.
 */
export interface Blob {
    /**
     * The image's media type. Anything outside the allowlist decodes as
     * [`OpenCode::Other`] and fails check C1 with `E.BLOB.MEDIATYPE`.
     */
    media_type: string;
    /**
     * The byte length of `bytes`; must match it exactly.
     */
    size: number;
    /**
     * Image width in pixels, if known.
     */
    width?: number;
    /**
     * Image height in pixels, if known.
     */
    height?: number;
    /**
     * The raw image bytes.
     */
    bytes: Uint8Array;
}

/**
 * An instant in time, as integer seconds since the Unix epoch (UTC).
 */
export type Timestamp = number;

/**
 * Facts about this build of the engine.
 */
export interface CoreInfo {
    /**
     * The format version implemented, e.g. `65536` for 1.0.0.
     */
    format_version: number;
    /**
     * The lowest `min_v` this build opens.
     */
    min_reader_version: number;
    /**
     * The `crisp-core` crate version.
     */
    core_version: string;
}

/**
 * How a reader wants a recipe shown.
 */
export interface ViewOptions {
    /**
     * Preferred languages, best first.
     */
    languages?: LanguageTag[];
    /**
     * The scale factor applied to every scalable amount.
     */
    scale?: Rational;
    /**
     * The measurement system amounts are shown in.
     */
    units?: Units;
}

/**
 * How much trust a verified bundle carries.
 */
export type Trust = "unsigned" | "author-signed" | "endorsed";

/**
 * Nutrition information (`crisp-v1.cddl`: `nutrition`).
 */
export interface Nutrition {
    /**
     * The basis the values are measured against, e.g. per serving.
     */
    basis: Quantity;
    /**
     * The nutrient values, keyed by nutrient code.
     */
    values: Record<string, Quantity>;
}

/**
 * One catalogued unit.
 */
export interface UnitInfo {
    /**
     * The UCUM code, e.g. `[tbsp_us]`.
     */
    code: string;
    /**
     * The dimension.
     */
    kind: UnitKind;
    /**
     * The measurement system.
     */
    system: UnitSystem;
    /**
     * The label for an amount of exactly one, e.g. `"tbsp"` or `"cup"`.
     */
    singular: string;
    /**
     * The label for any other amount, e.g. `"cups"`.
     */
    plural: string;
}

/**
 * One ingredient line.
 */
export interface IngredientView {
    /**
     * The ingredient's `ref` handle.
     */
    ref: number;
    /**
     * The name.
     */
    name: string;
    /**
     * The formatted amount (empty when none).
     */
    amount: string;
    /**
     * Equivalent measures, formatted.
     */
    alt: string[];
    /**
     * Preparation, e.g. "finely chopped".
     */
    preparation: string | undefined;
    /**
     * A short note.
     */
    note: string | undefined;
    /**
     * Whether it is optional.
     */
    optional: boolean;
    /**
     * Whether the amount ignores scaling.
     */
    fixed: boolean;
    /**
     * Substitutes, formatted as `"name (amount)"`.
     */
    substitutes: string[];
    /**
     * A component recipe id.
     */
    recipe: string | undefined;
    /**
     * An image blob id.
     */
    image: string | undefined;
}

/**
 * One piece of equipment.
 */
export interface EquipmentView {
    /**
     * The equipment's `ref` handle.
     */
    ref: number;
    /**
     * The name.
     */
    name: string;
    /**
     * How many, formatted (empty when unspecified).
     */
    amount: string;
    /**
     * A note, e.g. a size.
     */
    note: string | undefined;
    /**
     * An image blob id.
     */
    image: string | undefined;
}

/**
 * One problem a check found, located by a JSON Pointer into the payload.
 *
 * The verifier condenses a check's issues into one report line; editor
 * validation ([`crate::author::validate_draft`]) returns them all so a UI
 * can mark the offending fields.
 */
export interface Issue {
    /**
     * A JSON Pointer (RFC 6901) into the payload, e.g.
     * `/recipes/0/steps/2/timers/0/duration`. Empty for the whole document.
     */
    path: string;
    /**
     * The stable error code.
     */
    code: string;
    /**
     * A human-readable explanation.
     */
    message: string;
}

/**
 * One published recipe, as listed by browse and search.
 */
export interface RemoteRecipe {
    /**
     * The bundle's document id.
     */
    doc_id: string;
    /**
     * The recipe id.
     */
    recipe_id: string;
    /**
     * The revision id.
     */
    revision: string;
    /**
     * The title in the bundle's default language.
     */
    title: string;
    /**
     * A plain-text summary.
     */
    summary: string | undefined;
    /**
     * Author display names.
     */
    authors: string[];
    /**
     * Tags, cuisine, course, and diet codes.
     */
    tags: string[];
    /**
     * Declared allergens.
     */
    allergens: string[];
    /**
     * The identity language.
     */
    language: string;
    /**
     * When last modified (epoch seconds).
     */
    modified_at: number;
}

/**
 * One recipe in a bundle summary.
 */
export interface RecipeEntry {
    /**
     * The index into the bundle's recipes.
     */
    index: number;
    /**
     * The recipe id.
     */
    id: string;
    /**
     * The revision id.
     */
    revision: string;
    /**
     * The resolved title.
     */
    title: string;
    /**
     * The cover image blob id, if any.
     */
    cover: string | undefined;
}

/**
 * One step.
 */
export interface StepView {
    /**
     * The step's `ref` handle.
     */
    ref: number;
    /**
     * The 1-based step number.
     */
    number: number;
    /**
     * The step-group title, if grouped.
     */
    section: string | undefined;
    /**
     * The instruction.
     */
    text: ParagraphView[];
    /**
     * The ingredients used, with amounts.
     */
    ingredients: StepIngredientView[];
    /**
     * The equipment used, by name.
     */
    equipment: string[];
    /**
     * Timers.
     */
    timers: TimerView[];
    /**
     * Heat level, e.g. `"Medium-high heat"`.
     */
    heat: string | undefined;
    /**
     * Temperature, e.g. `"180°C oven"`.
     */
    temperature: string | undefined;
    /**
     * The intermediate result this step produces.
     */
    produces: string | undefined;
    /**
     * Tips and warnings.
     */
    note: ParagraphView[];
    /**
     * An image blob id.
     */
    image: string | undefined;
}

/**
 * The allowlisted raster image formats (`crisp-v1.cddl`: `blob.media_type`).
 *
 * Only raster formats are permitted — no SVG, video, audio, or fonts — to
 * keep the viewer attack surface minimal.
 */
export type MediaType = "image/png" | "image/jpeg" | "image/webp" | "image/avif";

/**
 * The bounds and vocabularies an editor must respect.
 */
export interface Constraints {
    /**
     * Named numeric limits (spec §12 and the CDDL bounds).
     */
    limits: Map<string, number>;
    /**
     * The registry codes for each open vocabulary.
     */
    course: string[];
    /**
     * Diet codes.
     */
    diet: string[];
    /**
     * Allergen codes.
     */
    allergens: string[];
    /**
     * Nutrient codes.
     */
    nutrients: string[];
    /**
     * Every catalogued unit, for unit pickers.
     */
    units: UnitInfo[];
}

/**
 * The compliance class a bundle achieved.
 */
export type ComplianceClass = "structural" | "published";

/**
 * The five identity fields frozen at a recipe's (or book's) birth
 * (`crisp-v1.cddl`: `identity`). The document id is derived from exactly
 * these fields, so they never change across edits.
 */
export interface Identity {
    /**
     * The author's key fingerprint (32 bytes); see
     * [`crate::identity::key_fingerprint`].
     */
    author_fpr: Uint8Array;
    /**
     * The original, normalized title.
     */
    title: string;
    /**
     * The language of `title`.
     */
    language: LanguageTag;
    /**
     * The creation instant (frozen).
     */
    created_at: Timestamp;
    /**
     * A 128-bit random nonce fixed at birth (16 bytes).
     */
    birth_nonce: Uint8Array;
}

/**
 * The kind of element a [`RunRef`] addresses.
 */
export type RunRefKind = "ingredient" | "step" | "equipment" | "recipe";

/**
 * The metadata common to every bundle (`crisp-v1.cddl`: `document-meta`).
 */
export interface DocumentMeta {
    /**
     * The document title.
     */
    title: LocalizedText;
    /**
     * The authors; at least one, at most 64.
     */
    authors: Author[];
    /**
     * An SPDX license identifier, or `"NONE"`.
     */
    license: string;
    /**
     * When the document was created (frozen at birth).
     */
    created_at: Timestamp;
    /**
     * When the document was last edited.
     */
    modified_at: Timestamp;
    /**
     * The frozen identity tuple — required if, and only if, the bundle is a
     * book.
     */
    identity?: Identity;
    /**
     * An optional description.
     */
    description?: LocalizedProse;
    /**
     * An optional cover image blob.
     */
    cover?: BlobId;
    /**
     * Namespaced extension data.
     */
    ext?: Record<string, unknown>;
}

/**
 * The outcome of a single verification check.
 */
export type CheckStatus = "pass" | "fail" | "skipped";

/**
 * The physical dimension a unit measures.
 */
export type UnitKind = "mass" | "volume" | "time" | "temperature" | "length";

/**
 * The result of one verification check (`crisp-v1.cddl`: `check-result`).
 */
export interface CheckResult {
    /**
     * The check id, e.g. `"A1"` or `"C3"`.
     */
    id: string;
    /**
     * Whether the check passed, failed, or was skipped.
     */
    status: CheckStatus;
    /**
     * The stable error code, present on failure.
     */
    code?: string;
    /**
     * A human-readable explanation, present on failure.
     */
    detail?: string;
}

/**
 * The result of parsing a typed amount.
 */
export interface ParsedAmount {
    /**
     * The parsed quantity.
     */
    quantity: Quantity;
    /**
     * Whether the unit came from the catalogue (`false` for annotations
     * like `{clove}` or when there is no unit).
     */
    unit_known: boolean;
}

/**
 * The signed contents of a bundle (`crisp-v1.cddl`: `bundle-payload`).
 *
 * `Eq` is not derived because the optional `ext` map holds arbitrary CBOR,
 * which is only `PartialEq`.
 */
export interface BundlePayload {
    /**
     * The format version, e.g. `0x010000` for v1.0.0.
     */
    v: number;
    /**
     * The minimum reader version required to open this bundle.
     */
    min_v: number;
    /**
     * Whether this bundle is a recipe or a book.
     */
    kind: Kind;
    /**
     * The document id; a recipe id or a book id, consistent with `kind`.
     */
    doc_id: DocId;
    /**
     * The bundle's default language.
     */
    default_language: LanguageTag;
    /**
     * Every language used anywhere in the bundle.
     */
    languages: LanguageTag[];
    /**
     * The document metadata.
     */
    meta: DocumentMeta;
    /**
     * The recipes; at least one.
     */
    recipes: Recipe[];
    /**
     * The book structure — present if, and only if, `kind` is `book`.
     */
    book?: BookStructure;
    /**
     * The content-addressed blob store, keyed by blob id.
     */
    blobs: Map<BlobId, Blob>;
    /**
     * Namespaced extension data, ignored by conformant readers.
     */
    ext?: Record<string, unknown>;
}

/**
 * The target of a [`RunRef`]: either a numeric within-recipe handle or a
 * recipe identifier.
 */
export type RunTarget = number | RecipeId;

/**
 * The top-level structure of a book bundle.
 */
export interface BookStructure {
    /**
     * Optional front-matter pages.
     */
    front_matter?: Page[];
    /**
     * The book's parts; at least one.
     */
    parts: Part[];
    /**
     * Optional back-matter pages.
     */
    back_matter?: Page[];
}

/**
 * What a [`Temperature`] applies to (`crisp-v1.cddl`: `temperature.target`).
 */
export type TemperatureTarget = "oven" | "internal" | "oil" | "pan" | "water";

/**
 * What a library list needs to know about a bundle.
 */
export interface BundleSummary {
    /**
     * The document id.
     */
    doc_id: string;
    /**
     * `"recipe"` or `"book"`.
     */
    kind: string;
    /**
     * The resolved document title.
     */
    title: string;
    /**
     * The recipes.
     */
    recipes: RecipeEntry[];
    /**
     * Author display names.
     */
    authors: string[];
    /**
     * Author ids, matching `authors`.
     */
    author_ids: string[];
    /**
     * Last modification (epoch seconds).
     */
    modified_at: number;
    /**
     * The bundle's languages.
     */
    languages: string[];
}

/**
 * What a recipe produces (`crisp-v1.cddl`: `yield`).
 */
export interface Yield {
    /**
     * The amount produced.
     */
    amount: Quantity;
    /**
     * The unit of the amount, e.g. "servings" or "cookies".
     */
    unit_text: LocalizedText;
}

/**
 * What the client needs from `GET /server-info`.
 */
export interface ServerInfoView {
    /**
     * The API versions the server speaks.
     */
    api_versions: string[];
    /**
     * The server's crisp-core version.
     */
    core_version: string;
    /**
     * The format version the server writes.
     */
    format_version: number;
    /**
     * The lowest format version it reads.
     */
    min_format_version: number;
    /**
     * The trusted endorsement keys, ready for `Bundle.verify`.
     */
    platform_keys: PlatformKey[];
    /**
     * The platform key's `usr…` id — the login audience.
     */
    platform_user_id: string;
}

/**
 * Where a recipe came from (`crisp-v1.cddl`: `source`).
 */
export interface Source {
    /**
     * A book or site name.
     */
    title?: string;
    /**
     * The original author.
     */
    author?: string;
    /**
     * An `https://` URL.
     */
    url?: string;
}

/**
 * Where the recipe came from.
 */
export interface SourceView {
    /**
     * A book or site name.
     */
    title: string | undefined;
    /**
     * The original author.
     */
    author: string | undefined;
    /**
     * An `https` URL.
     */
    url: string | undefined;
}

/**
 * Whether a bundle holds a single recipe or a whole book.
 */
export type Kind = "recipe" | "book";

/**
 * Which measurement system a reader wants amounts shown in.
 */
export type Units = "original" | "metric" | "us";

/**
 * Which measurement system a unit belongs to.
 */
export type UnitSystem = "metric" | "us" | "neutral";

/**
 * Whole-recipe durations (`crisp-v1.cddl`: `times`). Each uses a UCUM time
 * unit.
 */
export interface Times {
    /**
     * Hands-on preparation time.
     */
    prep?: Quantity;
    /**
     * Cooking time, active and passive.
     */
    cook?: Quantity;
    /**
     * Resting, chilling, or proofing time.
     */
    rest?: Quantity;
    /**
     * Start-to-plate time.
     */
    total?: Quantity;
}


/**
 * An opened `.crisp` file. Decoding is strict but does not verify; call
 * [`Bundle::verify`] for a compliance report.
 */
export class Bundle {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * A blob's media type and bytes, or `undefined`.
     *
     * # Errors
     *
     * Throws if serialization fails.
     */
    blob(id: string): BlobData | undefined;
    /**
     * The file bytes.
     */
    bytes(): Uint8Array;
    /**
     * The payload as an editable draft.
     *
     * # Errors
     *
     * Throws if serialization fails.
     */
    draft(): BundlePayload;
    /**
     * Opens `.crisp` file bytes.
     *
     * # Errors
     *
     * Throws (with a stable `code`) if the file cannot be decoded.
     */
    static open(bytes: Uint8Array): Bundle;
    /**
     * Summarizes the bundle in the first of `languages` it has.
     *
     * # Errors
     *
     * Throws if serialization fails.
     */
    summary(languages: string[]): BundleSummary;
    /**
     * Verifies the file against `platformKeys` (pass `[]` offline).
     *
     * # Errors
     *
     * Throws if the arguments are malformed.
     */
    verify(platformKeys: PlatformKey[], now: number): ComplianceReport;
    /**
     * Builds the render-ready view of recipe `index`.
     *
     * # Errors
     *
     * Throws if `index` is out of range or the options are malformed.
     */
    view(index: number, options: ViewOptions): RecipeView;
    /**
     * The document id.
     */
    readonly docId: string;
}

/**
 * A Crisp server at one base URL, with an optional session.
 */
export class CrispServer {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Lists published recipes, newest first.
     */
    browse(limit: number, offset: number): Promise<RemotePage>;
    /**
     * Requests a login challenge for `username`.
     */
    challenge(username: string): Promise<Challenge>;
    /**
     * Downloads a published file — the latest revision, or exactly
     * `revision`. Resolves to `null` if there is none.
     */
    download(doc_id: string, revision?: string | null): Promise<Uint8Array | null>;
    /**
     * Reads the server's versions and endorsement keys.
     */
    info(): Promise<ServerInfoView>;
    /**
     * Exchanges a signed challenge for a session.
     */
    login(nonce: string, signature: Uint8Array): Promise<Session>;
    /**
     * Ends the current session.
     */
    logout(): Promise<void>;
    /**
     * A server at `baseUrl` (e.g. `https://crisp.example`), signed out.
     */
    constructor(base_url: string);
    /**
     * Publishes an author-signed `.crisp` file; resolves to the endorsed
     * file bytes.
     */
    publish(bundle: Uint8Array): Promise<Uint8Array>;
    /**
     * Registers an account from `Signer.signProfile` bytes; resolves to
     * the account's `usr…` id.
     */
    register(signed_profile: Uint8Array): Promise<string>;
    /**
     * Searches published recipes.
     */
    search(query: SearchQuery): Promise<RemotePage>;
    /**
     * Uses `token` (or no session, for `null`) on later calls.
     */
    setToken(token?: string | null): void;
}

/**
 * An unlocked identity. Holds secret key material inside WASM memory;
 * call `free()` when locking.
 */
export class Signer {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Creates a brand-new random identity.
     *
     * # Errors
     *
     * Throws if the system RNG is unavailable.
     */
    static generate(): Signer;
    /**
     * Seals a draft into a signed `.crisp` file. Pass the bytes of the file
     * being edited as `previous`: the owner's edit becomes a revision,
     * anyone else's a fork.
     *
     * # Errors
     *
     * Throws with the draft's `issues` if it would not verify.
     */
    seal(draft: BundlePayload, displayName: string, now: number, previous?: Uint8Array | null): Uint8Array;
    /**
     * Answers a server login challenge (spec §8.7): signs the
     * audience-bound `crisp/login/1` message for `nonce` (hex).
     *
     * # Errors
     *
     * Throws if the nonce is not hex or signing fails.
     */
    signLogin(audience: string, nonce: string): Uint8Array;
    /**
     * Signs a self-asserted account profile for server registration.
     *
     * # Errors
     *
     * Throws if a field is out of range or signing fails.
     */
    signProfile(username: string, display_name: string, now: number): Uint8Array;
    /**
     * Unlocks a `.crispkey` file. Takes about a second (Argon2id, 64 MiB).
     *
     * # Errors
     *
     * Throws if the file is not a key file or the passphrase is wrong.
     */
    static unlock(key_file: Uint8Array, passphrase: string): Signer;
    /**
     * Wraps this identity under `passphrase` as `.crispkey` bytes.
     *
     * # Errors
     *
     * Throws if the passphrase is too short or encryption fails.
     */
    wrap(passphrase: string): Uint8Array;
    /**
     * This identity's `usr…` id.
     */
    readonly userId: string;
}

/**
 * Returns facts about this build of the engine.
 *
 * # Errors
 *
 * Throws if serialization fails.
 */
export function coreInfo(): CoreInfo;

/**
 * Formats a quantity for display (`"1½ cups"`).
 *
 * # Errors
 *
 * Throws if the quantity is malformed.
 */
export function formatQuantity(quantity: Quantity, lang: string): string;

/**
 * Wraps encoded image bytes as a content-addressed blob.
 *
 * # Errors
 *
 * Throws if the media type is not an allowlisted raster type or the image
 * is larger than a blob may be.
 */
export function imageBlob(bytes: Uint8Array, media_type: string, width: number, height: number): ImageBlob;

/**
 * Reads the `usr…` id of a key file without unlocking it.
 *
 * # Errors
 *
 * Throws if the bytes are not a supported key file.
 */
export function keyFileUserId(key_file: Uint8Array): string;

/**
 * Creates the identity of a new recipe (fresh birth nonce).
 *
 * # Errors
 *
 * Throws if the system RNG fails or `now` is not whole seconds.
 */
export function newIdentity(title: string, lang: string, now: number): Identity;

/**
 * Parses a typed amount (`"1 1/2 cups"`, `"to taste"`) in `lang`.
 *
 * # Errors
 *
 * Throws if the text is empty or its unit is unparseable.
 */
export function parseAmount(text: string, lang: string): ParsedAmount;

/**
 * Parses a typed duration (`"90"`, `"1h 30m"`, `"20-25 min"`).
 *
 * # Errors
 *
 * Throws if the text is not a duration.
 */
export function parseDuration(text: string): Quantity;

/**
 * Writes a quantity back as editable text (`"1 1/2 cups"`).
 *
 * # Errors
 *
 * Throws if the quantity is malformed.
 */
export function quantityToInput(quantity: Quantity, lang: string): string;

/**
 * Returns the bounds and vocabularies an editor must respect.
 *
 * # Errors
 *
 * Throws if serialization fails.
 */
export function recipeConstraints(): Constraints;

/**
 * Installs a panic hook that surfaces Rust panics in the browser console.
 */
export function start(): void;

/**
 * Validates an editor draft, returning every problem with its JSON Pointer.
 *
 * # Errors
 *
 * Throws if the draft does not even have the payload's shape.
 */
export function validateDraft(draft: BundlePayload): Issue[];

/**
 * Verifies a `signed-profile` and returns the profile it carries.
 *
 * # Errors
 *
 * Throws if the bytes are malformed or the signature is invalid.
 */
export function verifyProfile(bytes: Uint8Array): AccountProfile;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_bundle_free: (a: number, b: number) => void;
    readonly __wbg_crispserver_free: (a: number, b: number) => void;
    readonly __wbg_signer_free: (a: number, b: number) => void;
    readonly bundle_blob: (a: number, b: number, c: number) => [number, number, number];
    readonly bundle_bytes: (a: number) => [number, number];
    readonly bundle_docId: (a: number) => [number, number];
    readonly bundle_draft: (a: number) => [number, number, number];
    readonly bundle_open: (a: number, b: number) => [number, number, number];
    readonly bundle_summary: (a: number, b: number, c: number) => [number, number, number];
    readonly bundle_verify: (a: number, b: any, c: number) => [number, number, number];
    readonly bundle_view: (a: number, b: number, c: any) => [number, number, number];
    readonly coreInfo: () => [number, number, number];
    readonly crispserver_browse: (a: number, b: number, c: number) => any;
    readonly crispserver_challenge: (a: number, b: number, c: number) => any;
    readonly crispserver_download: (a: number, b: number, c: number, d: number, e: number) => any;
    readonly crispserver_info: (a: number) => any;
    readonly crispserver_login: (a: number, b: number, c: number, d: number, e: number) => any;
    readonly crispserver_logout: (a: number) => any;
    readonly crispserver_new: (a: number, b: number) => number;
    readonly crispserver_publish: (a: number, b: number, c: number) => any;
    readonly crispserver_register: (a: number, b: number, c: number) => any;
    readonly crispserver_search: (a: number, b: any) => any;
    readonly crispserver_setToken: (a: number, b: number, c: number) => void;
    readonly formatQuantity: (a: any, b: number, c: number) => [number, number, number, number];
    readonly imageBlob: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly keyFileUserId: (a: number, b: number) => [number, number, number, number];
    readonly newIdentity: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly parseAmount: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly parseDuration: (a: number, b: number) => [number, number, number];
    readonly quantityToInput: (a: any, b: number, c: number) => [number, number, number, number];
    readonly recipeConstraints: () => [number, number, number];
    readonly signer_generate: () => [number, number, number];
    readonly signer_seal: (a: number, b: any, c: number, d: number, e: number, f: number, g: number) => [number, number, number, number];
    readonly signer_signLogin: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly signer_signProfile: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly signer_unlock: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly signer_userId: (a: number) => [number, number];
    readonly signer_wrap: (a: number, b: number, c: number) => [number, number, number, number];
    readonly start: () => void;
    readonly validateDraft: (a: any) => [number, number, number];
    readonly verifyProfile: (a: number, b: number) => [number, number, number];
    readonly wasm_bindgen_6d2b1ea679af2a97___convert__closures_____invoke___wasm_bindgen_6d2b1ea679af2a97___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_6d2b1ea679af2a97___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_6d2b1ea679af2a97___convert__closures_____invoke___js_sys_6be0d7088fd65a6c___Function_fn_wasm_bindgen_6d2b1ea679af2a97___JsValue_____wasm_bindgen_6d2b1ea679af2a97___sys__Undefined___js_sys_6be0d7088fd65a6c___Function_fn_wasm_bindgen_6d2b1ea679af2a97___JsValue_____wasm_bindgen_6d2b1ea679af2a97___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen_6d2b1ea679af2a97___convert__closures_____invoke_______true_: (a: number, b: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
