/**
 * Durable storage: asks the browser not to evict the local library under
 * storage pressure.
 */

import { createLogger } from "../log";

const log = createLogger("store");

/** The slice of `StorageManager` used here (injectable for tests). */
export interface PersistableStorage {
  /** Whether storage is already persistent. */
  persisted?(): Promise<boolean>;
  /** Requests persistent storage. */
  persist?(): Promise<boolean>;
}

let pending: Promise<boolean> | null = null;

/**
 * Requests persistent storage once per page load. Resolves `true` when
 * storage is (or becomes) persistent; never rejects.
 */
export function requestPersistence(
  storage: PersistableStorage | undefined = globalThis.navigator?.storage,
): Promise<boolean> {
  if (pending) return pending;
  pending = (async () => {
    if (!storage?.persist) {
      log.debug("persist.unsupported");
      return false;
    }
    try {
      if (storage.persisted && (await storage.persisted())) {
        log.debug("persist.already");
        return true;
      }
      const granted = await storage.persist();
      log.info("persist.requested", { granted });
      return granted;
    } catch (error) {
      log.warn("persist.failed", { error: String(error) });
      return false;
    }
  })();
  return pending;
}

/** Forgets the memoized request (tests only). */
export function resetPersistenceRequest(): void {
  pending = null;
}
