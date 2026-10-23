import { expect, type Locator, type Page } from "@playwright/test";
import type { StoreKey, StoreNames, StoreValue } from "idb";
import {
  type CookRecord,
  type CrispSchema,
  cookKey,
  DB_NAME,
  DB_VERSION,
} from "../src/lib/store/schema";

// Shared end-to-end helpers. Everything that touches the app's IndexedDB
// store is bound to the store's own name and version, so a schema bump
// moves the suite with it instead of leaving literals behind.

/** The engine module's shape (the generated declarations). */
type Engine = typeof import("../src/generated/crisp");

/** Where the production build serves the crisp-core WASM engine. */
const ENGINE = {
  module: "/crisp-wasm/crisp.js",
  wasm: "/crisp-wasm/crisp_bg.wasm",
} as const;

/** A store of the app's database. */
type Store = StoreNames<CrispSchema>;

/** A form control by its exact accessible label. */
export const field = (page: Page, label: string): Locator =>
  page.getByLabel(label, { exact: true });

/** Waits until the service worker controls the page. */
export async function waitForServiceWorker(page: Page): Promise<void> {
  await page.waitForFunction(
    async () => {
      const registration = await navigator.serviceWorker.ready;
      return (
        registration.active !== null &&
        navigator.serviceWorker.controller !== null
      );
    },
    undefined,
    { timeout: 15_000 },
  );
}

/** Waits until the app has opened (and upgraded) its database. */
export async function waitForDatabase(page: Page): Promise<void> {
  await page.waitForFunction(
    async ({ name, version }) =>
      (await indexedDB.databases()).some(
        (db) => db.name === name && db.version === version,
      ),
    { name: DB_NAME, version: DB_VERSION },
  );
}

/** Writes one record into a store of the app's database. */
export async function putRecord<S extends Store>(
  page: Page,
  store: S,
  record: StoreValue<CrispSchema, S>,
): Promise<void> {
  await page.evaluate(
    ({ name, version, store, record }) =>
      new Promise<void>((resolve, reject) => {
        const open = indexedDB.open(name, version);
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const db = open.result;
          const tx = db.transaction(store, "readwrite");
          tx.objectStore(store).put(record);
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onerror = () => {
            db.close();
            reject(tx.error);
          };
        };
      }),
    { name: DB_NAME, version: DB_VERSION, store, record },
  );
}

/** Reads one record from a store of the app's database. */
export async function getRecord<S extends Store>(
  page: Page,
  store: S,
  key: StoreKey<CrispSchema, S>,
): Promise<StoreValue<CrispSchema, S> | undefined> {
  return page.evaluate(
    ({ name, version, store, key }) =>
      new Promise<StoreValue<CrispSchema, S> | undefined>((resolve, reject) => {
        const open = indexedDB.open(name, version);
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const db = open.result;
          const get = db.transaction(store).objectStore(store).get(key);
          get.onsuccess = () => {
            db.close();
            resolve(get.result);
          };
          get.onerror = () => {
            db.close();
            reject(get.error);
          };
        };
      }),
    { name: DB_NAME, version: DB_VERSION, store, key },
  );
}

/** Reads the saved cook-mode progress for one recipe revision. */
export function getCookRecord(
  page: Page,
  docId: string,
  revision: string,
): Promise<CookRecord | undefined> {
  return getRecord(page, "cook", cookKey(docId, revision));
}

/** A freshly generated author key, wrapped under a passphrase. */
export interface GeneratedIdentity {
  /** The wrapped key file bytes. */
  wrappedKey: Uint8Array;
  /** The `usr…` id the key belongs to. */
  userId: string;
}

/** Generates an author key in the page with the real engine. */
export async function generateIdentity(
  page: Page,
  passphrase: string,
): Promise<GeneratedIdentity> {
  return page.evaluate(
    async ({ engine, passphrase }) => {
      // A variable specifier: resolved by the browser, not by the bundler.
      const m: Engine = await import(engine.module);
      await m.default({ module_or_path: engine.wasm });
      const signer = m.Signer.generate();
      const identity = {
        wrappedKey: signer.wrap(passphrase),
        userId: signer.userId,
      };
      signer.free();
      return identity;
    },
    { engine: ENGINE, passphrase },
  );
}

/** What the engine reads from a `.crisp` file's summary. */
export interface OpenedFile {
  /** The document id. */
  docId: string;
  /** The first recipe's revision. */
  revision: string;
  /** The display title. */
  title: string;
}

/** Opens `.crisp` bytes in the page with the real engine. */
export async function openFile(
  page: Page,
  bytes: Uint8Array,
): Promise<OpenedFile> {
  return page.evaluate(
    async ({ engine, bytes }) => {
      const m: Engine = await import(engine.module);
      await m.default({ module_or_path: engine.wasm });
      const bundle = m.Bundle.open(bytes);
      const summary = bundle.summary(["en"]);
      bundle.free();
      return {
        docId: summary.doc_id,
        revision: summary.recipes[0]?.revision ?? "",
        title: summary.title,
      };
    },
    { engine: ENGINE, bytes },
  );
}

/** Saves the open editor, unlocking the identity; returns the doc id. */
export async function saveAndUnlock(
  page: Page,
  passphrase: string,
): Promise<string> {
  await page.getByRole("button", { name: "Save to library" }).click();
  const unlock = page.getByLabel("Passphrase to unlock your identity");
  await expect(unlock).toBeVisible();
  await unlock.fill(passphrase);
  await page.getByRole("button", { name: "Unlock and save" }).click();
  await expect(page).toHaveURL(/\/recipe\/rcp1[a-z0-9]+$/, { timeout: 20_000 });
  return decodeURIComponent(
    new URL(page.url()).pathname.split("/").pop() ?? "",
  );
}
