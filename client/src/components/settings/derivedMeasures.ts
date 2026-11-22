/**
 * The reader's "derived measures" setting: whether the recipe view shows a
 * density-derived measure (`≈ 140 g`) beside an ingredient whose author gave
 * only one. Off by default; the author's own measures are always shown.
 */

import { useSyncExternalStore } from "react";
import { createLogger } from "../../lib/log";

/** Where the choice is remembered. */
export const DERIVED_MEASURES_KEY = "crisp:derived-measures";

/** The stored value meaning "on"; anything else (or nothing) is off. */
const ON = "on";

const log = createLogger("settings");
const listeners = new Set<() => void>();

/** Whether the reader turned derived measures on. */
export function derivedMeasuresEnabled(): boolean {
  try {
    return globalThis.localStorage?.getItem(DERIVED_MEASURES_KEY) === ON;
  } catch {
    return false;
  }
}

/** Turns derived measures on or off, and tells every subscriber. */
export function setDerivedMeasures(enabled: boolean): void {
  try {
    if (enabled) globalThis.localStorage?.setItem(DERIVED_MEASURES_KEY, ON);
    else globalThis.localStorage?.removeItem(DERIVED_MEASURES_KEY);
  } catch {
    // Storage can be unavailable (private mode); the choice then reverts
    // to off on the next read.
  }
  log.info("settings.derived_measures", { enabled });
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  // Another tab changing the setting arrives as a `storage` event.
  const onStorage = (event: StorageEvent) => {
    if (event.key === DERIVED_MEASURES_KEY || event.key === null) listener();
  };
  globalThis.addEventListener?.("storage", onStorage);
  return () => {
    listeners.delete(listener);
    globalThis.removeEventListener?.("storage", onStorage);
  };
}

/** The setting, re-rendering when it changes. */
export function useDerivedMeasures(): boolean {
  return useSyncExternalStore(subscribe, derivedMeasuresEnabled, () => false);
}
