/**
 * The author identity: a random key generated in WASM, stored on this device
 * only as a passphrase-wrapped `.crispkey` file, and unlocked into memory
 * for authoring. Reading and cooking never need it.
 */

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { i18n } from "../i18n";
import { loadEngine, type Signer } from "./crisp";
import { createLogger } from "./log";
import { getStore, type KeyRecord } from "./store";

const log = createLogger("identity");

/** The single local identity's record id. */
const KEY_ID = "primary";

/** Where the identity is in its lifecycle. */
export type IdentityStatus = "loading" | "none" | "locked" | "unlocked";

/** The identity as the UI sees it. */
export interface IdentityState {
  /** Lifecycle status. */
  status: IdentityStatus;
  /** The `usr…` id, once a key exists. */
  userId: string | null;
  /** The name recipes are signed under. */
  displayName: string | null;
  /** The unlocked signer, while unlocked. */
  signer: Signer | null;
}

/** Everything the identity context offers. */
export interface IdentityContextValue extends IdentityState {
  /** Creates a new identity and unlocks it. Slow (key wrapping). */
  create(displayName: string, passphrase: string): Promise<void>;
  /** Unlocks the stored identity. Slow (key unwrapping). */
  unlock(passphrase: string): Promise<void>;
  /** Forgets the unlocked key; the stored file stays. */
  lock(): void;
  /** Replaces the stored identity with an imported `.crispkey` file. */
  importKeyFile(bytes: Uint8Array, displayName: string): Promise<void>;
  /** The stored `.crispkey` bytes, for backup. */
  exportKeyFile(): Promise<Uint8Array | null>;
  /** Changes the display name recipes are signed under. */
  rename(displayName: string): Promise<void>;
  /** Deletes the stored identity from this device. */
  forget(): Promise<void>;
}

const IdentityContext = createContext<IdentityContextValue | null>(null);

/** Lets the browser paint (e.g. a spinner) before a long synchronous call. */
export function nextPaint(): Promise<void> {
  return new Promise((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
  );
}

/** Provides the identity to the app. */
export function IdentityProvider({ children }: { children: ReactNode }) {
  const [record, setRecord] = useState<KeyRecord | null | undefined>(undefined);
  const [signer, setSigner] = useState<Signer | null>(null);

  useEffect(() => {
    void getStore()
      .then((store) => store.keys.get(KEY_ID))
      .then((stored) => setRecord(stored ?? null))
      .catch((error: unknown) => {
        log.error("load.failed", { error: String(error) });
        setRecord(null);
      });
  }, []);

  const replaceSigner = useCallback((next: Signer | null) => {
    setSigner((previous) => {
      if (previous && previous !== next) previous.free();
      return next;
    });
  }, []);

  const save = useCallback(async (next: KeyRecord) => {
    const store = await getStore();
    await store.keys.put(next);
    setRecord(next);
  }, []);

  const create = useCallback(
    async (displayName: string, passphrase: string) => {
      const engine = await loadEngine();
      await nextPaint();
      const fresh = engine.Signer.generate();
      const wrappedKey = fresh.wrap(passphrase);
      await save({
        id: KEY_ID,
        wrappedKey,
        userId: fresh.userId,
        displayName: displayName.trim(),
        updatedAt: Date.now(),
      });
      replaceSigner(fresh);
      log.info("created", { userId: fresh.userId });
    },
    [replaceSigner, save],
  );

  const unlock = useCallback(
    async (passphrase: string) => {
      if (!record) throw new Error(i18n.t("settings:identity.noneYet"));
      const engine = await loadEngine();
      await nextPaint();
      const unlocked = engine.Signer.unlock(record.wrappedKey, passphrase);
      replaceSigner(unlocked);
      log.info("unlocked", { userId: unlocked.userId });
    },
    [record, replaceSigner],
  );

  const lock = useCallback(() => replaceSigner(null), [replaceSigner]);

  const importKeyFile = useCallback(
    async (bytes: Uint8Array, displayName: string) => {
      const engine = await loadEngine();
      const userId = engine.keyFileUserId(bytes);
      replaceSigner(null);
      await save({
        id: KEY_ID,
        wrappedKey: bytes,
        userId,
        displayName: displayName.trim(),
        updatedAt: Date.now(),
      });
      log.info("imported", { userId });
    },
    [replaceSigner, save],
  );

  const exportKeyFile = useCallback(
    async () => record?.wrappedKey ?? null,
    [record],
  );

  const rename = useCallback(
    async (displayName: string) => {
      if (!record) return;
      await save({
        ...record,
        displayName: displayName.trim(),
        updatedAt: Date.now(),
      });
    },
    [record, save],
  );

  const forget = useCallback(async () => {
    const store = await getStore();
    await store.keys.delete(KEY_ID);
    replaceSigner(null);
    setRecord(null);
    log.info("forgotten");
  }, [replaceSigner]);

  const value = useMemo<IdentityContextValue>(() => {
    const status: IdentityStatus =
      record === undefined
        ? "loading"
        : record === null
          ? "none"
          : signer
            ? "unlocked"
            : "locked";
    return {
      status,
      userId: record?.userId ?? null,
      displayName: record?.displayName ?? null,
      signer,
      create,
      unlock,
      lock,
      importKeyFile,
      exportKeyFile,
      rename,
      forget,
    };
  }, [
    record,
    signer,
    create,
    unlock,
    lock,
    importKeyFile,
    exportKeyFile,
    rename,
    forget,
  ]);

  return (
    <IdentityContext.Provider value={value}>
      {children}
    </IdentityContext.Provider>
  );
}

/** The identity context. */
export function useIdentity(): IdentityContextValue {
  const value = useContext(IdentityContext);
  if (!value) throw new Error("useIdentity needs an <IdentityProvider>");
  return value;
}
