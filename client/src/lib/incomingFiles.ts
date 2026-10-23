/**
 * `.crisp` files arriving from the OS: the Web Share Target (stashed by the
 * service worker, see `plugins/sw-runtime.js`) and the File Handling API
 * (`launchQueue`, when the installed app is chosen to open a file).
 *
 * The bytes are untrusted — hand them to the WASM verifier like any import.
 */

import { createLogger } from "./log";

const log = createLogger("incoming");

/** The Cache Storage bucket the service worker writes shared files to. */
export const SHARE_INBOX = "crisp-share-inbox";

/** A file received from outside the app. */
export interface IncomingFile {
  /** The original file name. */
  name: string;
  /** The declared MIME type (may be empty or wrong). */
  type: string;
  /** The raw file bytes. */
  bytes: Uint8Array;
}

/** The slice of `CacheStorage` used. */
export interface CacheStorageLike {
  /** Opens a cache. */
  open(name: string): Promise<{
    keys(): Promise<readonly Request[]>;
    match(request: Request): Promise<Response | undefined>;
    delete(request: Request): Promise<boolean>;
  }>;
}

/**
 * Returns and clears files shared to the app (call on `/import?shared=1`).
 * Never rejects; returns `[]` when Cache Storage is unavailable.
 */
export async function takeSharedFiles(
  cachesApi: CacheStorageLike | undefined = globalThis.caches,
): Promise<IncomingFile[]> {
  if (!cachesApi) return [];
  try {
    const inbox = await cachesApi.open(SHARE_INBOX);
    const files: IncomingFile[] = [];
    for (const request of await inbox.keys()) {
      const response = await inbox.match(request);
      await inbox.delete(request);
      if (!response) continue;
      files.push({
        name: decodeURIComponent(
          response.headers.get("x-filename") ?? "shared.crisp",
        ),
        type: response.headers.get("content-type") ?? "",
        bytes: new Uint8Array(await response.arrayBuffer()),
      });
    }
    log.info("share.taken", { count: files.length });
    return files;
  } catch (error) {
    log.warn("share.read.failed", { error: String(error) });
    return [];
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/** Narrows an unknown value to a file handle with `getFile()`. */
function isFileHandle(value: unknown): value is { getFile(): Promise<File> } {
  return isRecord(value) && typeof value["getFile"] === "function";
}

/** `window.launchQueue` (not yet in the TS DOM types), if present. */
function globalLaunchQueue(): unknown {
  const scope: unknown = globalThis;
  return isRecord(scope) ? scope["launchQueue"] : undefined;
}

/**
 * Subscribes to files the OS asks the installed app to open (File Handling
 * API). Returns `false` when unsupported.
 */
export function onLaunchedFiles(
  consumer: (files: IncomingFile[]) => void,
  launchQueue: unknown = globalLaunchQueue(),
): boolean {
  if (!isRecord(launchQueue)) return false;
  const setConsumer = launchQueue["setConsumer"];
  if (typeof setConsumer !== "function") return false;
  setConsumer.call(launchQueue, async (params: unknown) => {
    const handles =
      isRecord(params) && Array.isArray(params["files"]) ? params["files"] : [];
    const files: IncomingFile[] = [];
    for (const handle of handles) {
      if (!isFileHandle(handle)) continue;
      try {
        const file = await handle.getFile();
        files.push({
          name: file.name,
          type: file.type,
          bytes: new Uint8Array(await file.arrayBuffer()),
        });
      } catch (error) {
        log.warn("launch.read.failed", { error: String(error) });
      }
    }
    log.info("launch.files", { count: files.length });
    if (files.length > 0) consumer(files);
  });
  return true;
}
