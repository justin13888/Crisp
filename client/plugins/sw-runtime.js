/*
 * Crisp service worker runtime.
 *
 * `plugins/sw.ts` prepends two constants at build time:
 *   PRECACHE_URLS — every emitted asset plus public/ files (incl. /crisp-wasm/)
 *   CACHE_NAME    — derived from a content hash of that list
 *
 * Strategy:
 *  - install:  precache everything, so the app works offline after one visit;
 *  - activate: delete older Crisp caches, then claim open pages;
 *  - fetch:    hashed static assets, precached files and /crisp-wasm/ are
 *              cache-first; navigations are network-first with a 3 s timeout,
 *              falling back to the cached /index.html shell; /api/ and
 *              cross-origin requests are never touched;
 *  - share:    a Web Share Target POST to /import stashes the shared files
 *              in the share inbox and redirects to /import?shared=1.
 *
 * A new worker waits until the page asks it to take over (SKIP_WAITING), so
 * an update never swaps code under a cook mid-recipe.
 */

const NAV_TIMEOUT_MS = 3000;
const SHELL_URL = "/index.html";
/** Old precaches to prune: generated ones plus the legacy hand-written one. */
const STALE = (name) =>
  name !== CACHE_NAME &&
  (name.startsWith("crisp-precache-") || name === "crisp-shell-v1");
/** Files received via the Web Share Target, read by the /import screen. */
const SHARE_INBOX = "crisp-share-inbox";
const SHARE_FIELD = "bundle";
const PRECACHED = new Set(PRECACHE_URLS);

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE_NAME)
      .then((cache) =>
        cache.addAll(
          PRECACHE_URLS.map((url) => new Request(url, { cache: "reload" })),
        ),
      ),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(names.filter(STALE).map((name) => caches.delete(name))),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("message", (event) => {
  if (event.data && event.data.type === "SKIP_WAITING") self.skipWaiting();
});

/** Serves from the precache, falling back to (and filling from) network. */
async function cacheFirst(request) {
  const cache = await caches.open(CACHE_NAME);
  const cached = await cache.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok) await cache.put(request, response.clone());
  return response;
}

/** Network first; after a timeout or failure, the cached app shell. */
async function networkFirstNavigation(request) {
  const network = fetch(request);
  const timeout = new Promise((resolve) => {
    setTimeout(() => resolve(null), NAV_TIMEOUT_MS);
  });
  try {
    const response = await Promise.race([network, timeout]);
    if (response) return response;
  } catch {
    // Offline or network error: fall through to the shell.
  }
  const shell = await caches.match(SHELL_URL, { cacheName: CACHE_NAME });
  return shell || network;
}

/** Whether a same-origin path is served cache-first. */
function isCacheFirst(pathname) {
  return (
    PRECACHED.has(pathname) ||
    pathname.startsWith("/static/") ||
    pathname.startsWith("/crisp-wasm/")
  );
}

/** Stores shared files for the /import screen, then redirects there. */
async function receiveShare(request) {
  try {
    const form = await request.formData();
    const inbox = await caches.open(SHARE_INBOX);
    let index = 0;
    for (const file of form.getAll(SHARE_FIELD)) {
      if (typeof file === "string") continue;
      await inbox.put(
        `/share-inbox/${Date.now()}-${index++}`,
        new Response(file, {
          headers: {
            "content-type": file.type || "application/crisp+cbor",
            "x-filename": encodeURIComponent(file.name),
          },
        }),
      );
    }
  } catch {
    // Fall through: the import screen simply shows no shared files.
  }
  return Response.redirect(
    new URL("/import?shared=1", self.location.origin).href,
    303,
  );
}

self.addEventListener("fetch", (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  if (request.method === "POST" && url.pathname === "/import") {
    event.respondWith(receiveShare(request));
    return;
  }
  if (request.method !== "GET") return;
  if (url.pathname.startsWith("/api/")) return;
  if (request.mode === "navigate") {
    event.respondWith(networkFirstNavigation(request));
  } else if (isCacheFirst(url.pathname)) {
    event.respondWith(cacheFirst(request));
  }
});
