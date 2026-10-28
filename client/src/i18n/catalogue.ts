/**
 * The shape of the string catalogue.
 *
 * English (`en/`) is the source of truth: its keys are the keys `t()`
 * accepts. Every other locale supplies a {@link Translation} of each
 * namespace, which the type checker holds to every English key. Plural
 * forms use i18next's `_one` / `_other` suffixes; a locale whose plural
 * rules need more forms (French `_many`) may add them.
 */
import type { en } from "./en";

/** The English catalogue's type. */
export type Catalogue = typeof en;

/** A catalogue namespace: one area of the app. */
export type Namespace = keyof Catalogue;

/** Every namespace, in the order i18next loads them. */
export const NAMESPACES = [
  "common",
  "library",
  "recipe",
  "cook",
  "editor",
  "settings",
  "browse",
] as const satisfies readonly Namespace[];

/** One locale's strings for a namespace: every English key, plus extras. */
export type Translation<N extends Namespace> = {
  readonly [K in keyof Catalogue[N]]: string;
} & Readonly<Record<string, string>>;
