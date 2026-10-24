/**
 * Autosaved editor drafts in the local store.
 *
 * Draft ids: `new` for the recipe being created at `/new`, and
 * `edit:<docId>` for unsaved changes to a stored recipe at `/edit/<docId>`.
 * The Library lists drafts with {@link describeDraft} and links them with
 * {@link hrefForDraft}.
 */

import { isRecord } from "../../lib/isRecord";
import { createLogger } from "../../lib/log";
import {
  type DraftRecord,
  type DraftRepository,
  getStore,
} from "../../lib/store";
import { FORM_VERSION, type FormState } from "./model";

const log = createLogger("editor.drafts");

/** The draft id of the recipe being created at `/new`. */
export const NEW_DRAFT_ID = "new";

const EDIT_PREFIX = "edit:";

/** The draft id for a stored recipe, or for a new one when `docId` is absent. */
export function draftIdFor(docId?: string): string {
  return docId ? `${EDIT_PREFIX}${docId}` : NEW_DRAFT_ID;
}

/** The editor route that resumes a draft. */
export function hrefForDraft(id: string): string {
  return id.startsWith(EDIT_PREFIX)
    ? `/edit/${encodeURIComponent(id.slice(EDIT_PREFIX.length))}`
    : "/new";
}

/** What the drafts store holds for one draft. */
interface StoredDraft {
  form: FormState;
}

const isString = (v: unknown): v is string => typeof v === "string";
const isStringArray = (v: unknown): v is string[] =>
  Array.isArray(v) && v.every(isString);

function hasFields(
  value: unknown,
  fields: Record<string, (v: unknown) => boolean>,
): boolean {
  if (!isRecord(value)) return false;
  return Object.entries(fields).every(([key, check]) => check(value[key]));
}

const isRef = (v: unknown) => typeof v === "number" && Number.isInteger(v);
const isGroupRef = (v: unknown) => v === null || isRef(v);
const isBool = (v: unknown) => typeof v === "boolean";
const listOf =
  (check: (v: unknown) => boolean) =>
  (v: unknown): boolean =>
    Array.isArray(v) && v.every(check);

const isGroup = (v: unknown) => hasFields(v, { key: isRef, name: isString });
const isIngredient = (v: unknown) =>
  hasFields(v, {
    ref: isRef,
    group: isGroupRef,
    name: isString,
    amount: isString,
    preparation: isString,
    note: isString,
    optional: isBool,
    fixed: isBool,
    alt: isStringArray,
    substitutes: listOf((s) =>
      hasFields(s, { name: isString, amount: isString }),
    ),
  });
const isEquipment = (v: unknown) =>
  hasFields(v, {
    ref: isRef,
    name: isString,
    quantity: isString,
    note: isString,
  });
const isStep = (v: unknown) =>
  hasFields(v, {
    ref: isRef,
    group: isGroupRef,
    text: isString,
    ingredients: listOf((p) => hasFields(p, { ref: isRef, amount: isString })),
    equipment: listOf(isRef),
    timers: listOf((t) =>
      hasFields(t, { duration: isString, label: isString, passive: isBool }),
    ),
    heat: isString,
    tempMin: isString,
    tempMax: isString,
    tempUnit: isString,
    tempTarget: isString,
    produces: isString,
    note: isString,
    image: (x) => x === null || isString(x),
  });

/** Narrows stored (untrusted) editor state to a {@link FormState}. */
export function isFormState(value: unknown): value is FormState {
  return hasFields(value, {
    v: (v) => v === FORM_VERSION,
    lang: isString,
    base: (v) => v === null || isRecord(v),
    identity: (v) =>
      hasFields(v, {
        title: isString,
        language: isString,
        created_at: (x) => typeof x === "number",
        author_fpr: (x) => x instanceof Uint8Array,
        birth_nonce: (x) => x instanceof Uint8Array,
      }),
    title: isString,
    summary: isString,
    yieldAmount: isString,
    yieldUnit: isString,
    times: (v) =>
      hasFields(v, {
        prep: isString,
        cook: isString,
        rest: isString,
        total: isString,
      }),
    ingredientGroups: listOf(isGroup),
    ingredients: listOf(isIngredient),
    equipment: listOf(isEquipment),
    stepGroups: listOf(isGroup),
    steps: listOf(isStep),
    media: isStringArray,
    blobs: isRecord,
    course: isStringArray,
    diet: isStringArray,
    allergens: isStringArray,
    cuisine: isStringArray,
    tags: isStringArray,
    notes: isString,
    source: (v) =>
      hasFields(v, { title: isString, author: isString, url: isString }),
    license: isString,
    nextRef: isRef,
  });
}

function formOf(record: DraftRecord | undefined): FormState | null {
  if (!record) return null;
  const state: unknown = record.state;
  if (!isRecord(state)) return null;
  return isFormState(state["form"]) ? state["form"] : null;
}

async function repository(drafts?: DraftRepository): Promise<DraftRepository> {
  return drafts ?? (await getStore()).drafts;
}

/** Loads an autosaved draft; `null` if there is none or it is unreadable. */
export async function loadDraft(
  id: string,
  drafts?: DraftRepository,
): Promise<FormState | null> {
  const record = await (await repository(drafts)).get(id);
  const form = formOf(record);
  if (record && !form) log.warn("draft.unreadable", { id });
  log.debug("draft.load", { id, found: Boolean(form) });
  return form;
}

/** Autosaves editor state under `id`. */
export async function saveDraft(
  id: string,
  form: FormState,
  drafts?: DraftRepository,
): Promise<void> {
  const state: StoredDraft = { form };
  await (await repository(drafts)).put({ id, state, updatedAt: Date.now() });
  log.debug("draft.save", {
    id,
    ingredients: form.ingredients.length,
    steps: form.steps.length,
  });
}

/** Deletes an autosaved draft (no-op if absent). */
export async function deleteDraft(
  id: string,
  drafts?: DraftRepository,
): Promise<void> {
  await (await repository(drafts)).delete(id);
  log.debug("draft.delete", { id });
}

/** A draft as the Library lists it. */
export interface DraftSummary {
  /** The draft id. */
  id: string;
  /** The recipe title typed so far (may be empty). */
  title: string;
  /** Whether it revises a stored recipe (vs. a new one). */
  editing: boolean;
  /** The editor route that resumes it. */
  href: string;
  /** When it was last autosaved (epoch ms). */
  updatedAt: number;
}

/** Describes a stored draft record for listing; `null` if unreadable. */
export function describeDraft(record: DraftRecord): DraftSummary | null {
  const form = formOf(record);
  if (!form) return null;
  return {
    id: record.id,
    title: form.title.trim(),
    editing: record.id.startsWith(EDIT_PREFIX),
    href: hrefForDraft(record.id),
    updatedAt: record.updatedAt,
  };
}
