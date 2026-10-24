/**
 * The editor's React context: the form, its updater, the engine, and the
 * validation problems visible per field.
 */

import { createContext, useContext } from "react";
import type { Engine } from "../../lib/crisp";
import type { Created } from "./actions";
import type { FormState } from "./model";

/** Classification vocabularies and bounds from `recipeConstraints()`. */
export interface Vocabularies {
  /** Course codes. */
  course: string[];
  /** Diet codes. */
  diet: string[];
  /** Allergen codes. */
  allergens: string[];
  /**
   * The longest free-form slug (cuisine, tag), in bytes; unbounded until the
   * engine loads (`validateDraft` still reports an over-long slug).
   */
  maxSlugBytes: number;
}

/** Everything editor components share. */
export interface EditorContextValue {
  /** The loaded engine. */
  engine: Engine;
  /** The form being edited. */
  form: FormState;
  /** Applies a pure change to the form (marks it dirty). */
  update(change: (form: FormState) => FormState): void;
  /** Applies a change that creates an item; returns the new ref or key. */
  create(change: (form: FormState) => Created): number;
  /** Visible problem messages, by field id. */
  problems: ReadonlyMap<string, string[]>;
  /** Marks a field as visited, so its problems show. */
  touch(fieldId: string): void;
  /** Focuses a field after the next render (e.g. a just-added row). */
  focusLater(fieldId: string): void;
  /** Classification vocabularies. */
  vocab: Vocabularies;
}

const EditorContext = createContext<EditorContextValue | null>(null);

/** Provides the editor context. */
export const EditorProvider = EditorContext.Provider;

/** The editor context. */
export function useEditor(): EditorContextValue {
  const value = useContext(EditorContext);
  if (!value) throw new Error("useEditor needs an <EditorProvider>");
  return value;
}

/** Focuses a field by id, opening any collapsed panel around it. */
export function focusField(fieldId: string): boolean {
  const element = document.getElementById(fieldId);
  if (!element) return false;
  for (
    let parent = element.parentElement;
    parent;
    parent = parent.parentElement
  ) {
    if (parent instanceof HTMLDetailsElement) parent.open = true;
  }
  element.scrollIntoView({ block: "center", behavior: "smooth" });
  element.focus({ preventScroll: true });
  return true;
}
