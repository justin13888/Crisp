/*
 * Crisp service worker — offline-first app shell.
 *
 * Strategy:
 *  - app shell (HTML/JS/CSS): network-first, falling back to the cache, so
 *    the app keeps working fully offline once it has been opened once;
 *  - the `crisp-core` WASM module: cache-first (it is large and immutable
 *    per build);
 *  - everything else (e.g. API calls): passed straight through to the
 *    network — recipe data is cached deliberately by the app in IndexedDB,
 *    not opportunistically here.
 */

const CACHE = "crisp-shell-v1";
const SHELL = ["/", "/index.html", "/manifest.webmanifest", "/icon.svg"];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(SHELL))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(
          keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)),
        ),
      )
      .then(() => self.clients.claim()),
  );
});

async function cacheFirst(request) {
  const cached = await caches.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok) {
    const cache = await caches.open(CACHE);
    cache.put(request, response.clone());
  }
  return response;
}

async function networkFirst(request) {
  try {
    const response = await fetch(request);
    if (response.ok) {
      const cache = await caches.open(CACHE);
      cache.put(request, response.clone());
    }
    return response;
  } catch (error) {
    const cached = await caches.match(request);
    if (cached) return cached;
    const shell = await caches.match("/index.html");
    if (shell) return shell;
    throw error;
  }
}

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  if (url.pathname.startsWith("/crisp-wasm/")) {
    event.respondWith(cacheFirst(request));
  } else if (
    request.mode === "navigate" ||
    url.pathname.startsWith("/static/")
  ) {
    event.respondWith(networkFirst(request));
  }
});
