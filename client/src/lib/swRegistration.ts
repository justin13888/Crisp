/**
 * Service-worker registration and the "update available" signal.
 *
 * A new worker installs in the background and then waits; the UI shows an
 * "update available — reload" prompt and calls {@link applyUpdate}, which
 * tells the waiting worker to take over and reloads once it controls the
 * page. Nothing swaps under a cook mid-recipe.
 */

import { createLogger } from "./log";

const log = createLogger("sw");

/** The slice of `ServiceWorker` used. */
export interface WorkerLike extends EventTarget {
  /** Lifecycle state, e.g. `installed`. */
  readonly state: string;
  /** Sends a message to the worker. */
  postMessage(message: unknown): void;
}

/** The slice of `ServiceWorkerRegistration` used. */
export interface RegistrationLike extends EventTarget {
  /** A worker that is installing. */
  readonly installing: WorkerLike | null;
  /** A worker installed and waiting to activate. */
  readonly waiting: WorkerLike | null;
  /** Checks the server for a newer worker. */
  update(): Promise<unknown>;
}

/** The slice of `ServiceWorkerContainer` used. */
export interface ContainerLike extends EventTarget {
  /** The worker controlling this page, if any. */
  readonly controller: unknown;
  /** Registers a worker script. */
  register(
    url: string,
    options?: RegistrationOptions,
  ): Promise<RegistrationLike>;
}

/** Page hooks (injectable for tests). */
export interface PageLike {
  /** Reloads the page. */
  reload(): void;
  /** Subscribes to visibility changes; returns whether visible. */
  onVisible(listener: () => void): void;
}

type Listener = () => void;

let waiting: WorkerLike | null = null;
let reloading = false;
const listeners = new Set<Listener>();

function setWaiting(worker: WorkerLike | null): void {
  if (worker === waiting) return;
  waiting = worker;
  log.info("update.available", { available: worker !== null });
  for (const listener of listeners) listener();
}

/** Whether a new version is installed and waiting. */
export function isUpdateAvailable(): boolean {
  return waiting !== null;
}

/** Subscribes to update availability changes. */
export function onUpdateAvailable(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Activates the waiting worker; the page reloads once it takes control. */
export function applyUpdate(): void {
  if (!waiting) return;
  reloading = true;
  log.info("update.apply", {});
  waiting.postMessage({ type: "SKIP_WAITING" });
}

function browserPage(): PageLike {
  return {
    reload: () => window.location.reload(),
    onVisible: (listener) => {
      document.addEventListener("visibilitychange", () => {
        if (document.visibilityState === "visible") listener();
      });
    },
  };
}

/**
 * Registers the worker immediately (not on `load`, so the precache starts as
 * early as possible). Resolves to the registration, or `null` when
 * unsupported or failed — the app works online either way.
 */
export async function registerServiceWorker(
  container: ContainerLike | undefined = globalThis.navigator?.serviceWorker,
  page: PageLike = browserPage(),
  url = "/sw.js",
): Promise<RegistrationLike | null> {
  if (!container) {
    log.debug("unsupported", {});
    return null;
  }
  // A page with no controller is a first install: no update prompt needed.
  const isUpdate = (): boolean => Boolean(container.controller);

  container.addEventListener("controllerchange", () => {
    log.info("controller.changed", { reloading });
    if (reloading) page.reload();
  });

  let registration: RegistrationLike;
  try {
    registration = await container.register(url, { scope: "/" });
  } catch (error) {
    log.warn("register.failed", { error: String(error) });
    return null;
  }
  log.info("registered", { url });

  if (registration.waiting && isUpdate()) setWaiting(registration.waiting);
  registration.addEventListener("updatefound", () => {
    const worker = registration.installing;
    if (!worker) return;
    log.debug("update.found", {});
    worker.addEventListener("statechange", () => {
      if (worker.state === "installed" && isUpdate()) setWaiting(worker);
    });
  });

  // Long-lived kitchen tablets: look for updates whenever the app is shown.
  page.onVisible(() => {
    registration.update().catch((error: unknown) => {
      log.debug("update.check.failed", { error: String(error) });
    });
  });
  return registration;
}

/** Resets module state (tests only). */
export function resetServiceWorkerState(): void {
  waiting = null;
  reloading = false;
  listeners.clear();
}
