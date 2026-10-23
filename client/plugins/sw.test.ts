import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  buildPrecache,
  listPublicFiles,
  type Precache,
  renderServiceWorker,
} from "./sw";

const runtime = await Bun.file(
  new URL("./sw-runtime.js", import.meta.url),
).text();

describe("buildPrecache", () => {
  test("sorts, de-duplicates and excludes non-cacheable files", () => {
    const p = buildPrecache([
      { url: "/static/js/b.js", content: "b" },
      { url: "/index.html", content: "<html>" },
      { url: "/static/js/b.js", content: "b" },
      { url: "/static/js/b.js.map", content: "{}" },
      { url: "/static/js/b.js.LICENSE.txt", content: "MIT" },
      { url: "/sw.js", content: "old" },
      { url: "/crisp-wasm/.gitignore", content: "*" },
      {
        url: "/crisp-wasm/crisp_core_bg.wasm",
        content: new Uint8Array([0, 97]),
      },
    ]);
    expect(p.urls).toEqual([
      "/crisp-wasm/crisp_core_bg.wasm",
      "/index.html",
      "/static/js/b.js",
    ]);
    expect(p.cacheName).toMatch(/^crisp-precache-[0-9a-f]{16}$/);
  });

  test("the cache name changes with any content and is order-independent", () => {
    const a = { url: "/a.js", content: "1" };
    const b = { url: "/b.js", content: "2" };
    const base = buildPrecache([a, b]).cacheName;
    expect(buildPrecache([b, a]).cacheName).toBe(base);
    expect(buildPrecache([a, { ...b, content: "3" }]).cacheName).not.toBe(base);
    expect(buildPrecache([a]).cacheName).not.toBe(base);
  });
});

describe("listPublicFiles", () => {
  test("walks nested directories into URL paths", () => {
    const dir = mkdtempSync(join(tmpdir(), "crisp-public-"));
    mkdirSync(join(dir, "crisp-wasm"));
    writeFileSync(join(dir, "manifest.webmanifest"), "{}");
    writeFileSync(join(dir, "crisp-wasm", "crisp_core.js"), "x");
    expect(listPublicFiles(dir).map((f) => f.url)).toEqual([
      "/crisp-wasm/crisp_core.js",
      "/manifest.webmanifest",
    ]);
    expect(listPublicFiles(join(dir, "missing"))).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// Behavioural tests: run the generated worker against fake browser globals.

type Handler = (event: Record<string, unknown>) => void;

interface Harness {
  dispatch(type: string, event: Record<string, unknown>): void;
  caches: Map<string, Map<string, Response>>;
  fetched: string[];
  claimed: () => boolean;
  skipped: () => boolean;
  setNetwork(fn: (url: string) => Promise<Response>): void;
}

function harness(precache: Precache): Harness {
  const handlers = new Map<string, Handler>();
  const stores = new Map<string, Map<string, Response>>();
  const fetched: string[] = [];
  let network: (url: string) => Promise<Response> = async (url) =>
    new Response(`net:${new URL(url).pathname}`);
  let claimed = false;
  let skipped = false;

  const key = (input: Request | string): string =>
    new URL(typeof input === "string" ? input : input.url, "https://crisp.test")
      .pathname;
  const cacheFor = (name: string) => {
    let store = stores.get(name);
    if (!store) {
      store = new Map();
      stores.set(name, store);
    }
    const s = store;
    return {
      match: async (req: Request | string) => s.get(key(req))?.clone(),
      put: async (req: Request | string, res: Response) => {
        s.set(key(req), res);
      },
      addAll: async (reqs: (Request | string)[]) => {
        for (const req of reqs) {
          const url = new URL(key(req), "https://crisp.test").href;
          s.set(key(req), await fakeFetch(url));
        }
      },
    };
  };
  const fakeFetch = async (input: Request | string): Promise<Response> => {
    const url = typeof input === "string" ? input : input.url;
    fetched.push(new URL(url).pathname);
    return network(url);
  };
  const fakeCaches = {
    open: async (name: string) => cacheFor(name),
    keys: async () => [...stores.keys()],
    delete: async (name: string) => stores.delete(name),
    match: async (req: string, opts?: { cacheName?: string }) =>
      opts?.cacheName ? cacheFor(opts.cacheName).match(req) : undefined,
  };
  const self = {
    location: { origin: "https://crisp.test" },
    addEventListener: (type: string, fn: Handler) => handlers.set(type, fn),
    skipWaiting: () => {
      skipped = true;
    },
    clients: {
      claim: async () => {
        claimed = true;
      },
    },
  };
  const code = renderServiceWorker(precache, runtime);
  // In a worker, relative URLs resolve against the worker's location.
  class WorkerRequest extends Request {
    constructor(input: string, init?: RequestInit) {
      super(new URL(input, "https://crisp.test").href, init);
    }
  }
  new Function("self", "caches", "fetch", "Request", code)(
    self,
    fakeCaches,
    fakeFetch,
    WorkerRequest,
  );
  return {
    dispatch: (type, event) => {
      const handler = handlers.get(type);
      if (!handler) throw new Error(`no ${type} handler`);
      handler(event);
    },
    caches: stores,
    fetched,
    claimed: () => claimed,
    skipped: () => skipped,
    setNetwork: (fn) => {
      network = fn;
    },
  };
}

async function waitUntil(h: Harness, type: string): Promise<void> {
  let pending: Promise<unknown> = Promise.resolve();
  h.dispatch(type, {
    waitUntil: (p: Promise<unknown>) => {
      pending = p;
    },
  });
  await pending;
}

/** Dispatches a fetch; resolves to the response, or null if not handled. */
async function request(
  h: Harness,
  path: string,
  init: { mode?: string; method?: string; origin?: string } = {},
): Promise<Response | null> {
  let responded: Promise<Response> | null = null;
  h.dispatch("fetch", {
    request: {
      url: `${init.origin ?? "https://crisp.test"}${path}`,
      method: init.method ?? "GET",
      mode: init.mode ?? "cors",
    },
    respondWith: (p: Promise<Response>) => {
      responded = p;
    },
  });
  return responded ? await responded : null;
}

const precache = buildPrecache([
  { url: "/index.html", content: "<html>" },
  { url: "/static/js/index.abc.js", content: "js" },
  { url: "/crisp-wasm/crisp_core_bg.wasm", content: "wasm" },
  { url: "/manifest.webmanifest", content: "{}" },
]);

describe("generated service worker", () => {
  test("install precaches everything; activate prunes old caches and claims", async () => {
    const h = harness(precache);
    h.caches.set("crisp-shell-v1", new Map());
    h.caches.set("crisp-precache-old", new Map());
    h.caches.set("someone-else", new Map());
    h.caches.set("crisp-share-inbox", new Map());
    await waitUntil(h, "install");
    expect(
      [...(h.caches.get(precache.cacheName)?.keys() ?? [])].sort(),
    ).toEqual(precache.urls);
    await waitUntil(h, "activate");
    expect([...h.caches.keys()].sort()).toEqual(
      [precache.cacheName, "crisp-share-inbox", "someone-else"].sort(),
    );
    expect(h.claimed()).toBe(true);
    expect(h.skipped()).toBe(false);
  });

  test("waits for SKIP_WAITING before taking over", () => {
    const h = harness(precache);
    h.dispatch("message", { data: { type: "other" } });
    expect(h.skipped()).toBe(false);
    h.dispatch("message", { data: { type: "SKIP_WAITING" } });
    expect(h.skipped()).toBe(true);
  });

  test("serves static assets and WASM cache-first", async () => {
    const h = harness(precache);
    await waitUntil(h, "install");
    h.fetched.length = 0;
    h.setNetwork(async () => {
      throw new TypeError("offline");
    });
    expect(await (await request(h, "/static/js/index.abc.js"))?.text()).toBe(
      "net:/static/js/index.abc.js",
    );
    expect(
      await (await request(h, "/crisp-wasm/crisp_core_bg.wasm"))?.text(),
    ).toBe("net:/crisp-wasm/crisp_core_bg.wasm");
    expect(h.fetched).toEqual([]);
  });

  test("fills the cache for uncached /crisp-wasm/ files", async () => {
    const h = harness(precache);
    await waitUntil(h, "install");
    await request(h, "/crisp-wasm/extra.js");
    expect(h.caches.get(precache.cacheName)?.has("/crisp-wasm/extra.js")).toBe(
      true,
    );
  });

  test("navigations prefer the network", async () => {
    const h = harness(precache);
    await waitUntil(h, "install");
    const res = await request(h, "/recipe/abc", { mode: "navigate" });
    expect(await res?.text()).toBe("net:/recipe/abc");
  });

  test("offline navigations fall back to the cached shell", async () => {
    const h = harness(precache);
    await waitUntil(h, "install");
    h.setNetwork(async () => {
      throw new TypeError("offline");
    });
    const res = await request(h, "/recipe/abc", { mode: "navigate" });
    expect(await res?.text()).toBe("net:/index.html");
  });

  test("slow navigations fall back to the shell after 3 s", async () => {
    const h = harness(precache);
    await waitUntil(h, "install");
    h.setNetwork(() => new Promise(() => undefined)); // never answers
    const started = performance.now();
    const res = await request(h, "/", { mode: "navigate" });
    expect(await res?.text()).toBe("net:/index.html");
    expect(performance.now() - started).toBeGreaterThanOrEqual(2900);
  }, 6000);

  test("never handles /api/, cross-origin or non-GET requests", async () => {
    const h = harness(precache);
    await waitUntil(h, "install");
    expect(await request(h, "/api/recipes")).toBeNull();
    expect(await request(h, "/api/recipes", { mode: "navigate" })).toBeNull();
    expect(
      await request(h, "/static/js/index.abc.js", {
        origin: "https://cdn.example",
      }),
    ).toBeNull();
    expect(
      await request(h, "/static/js/index.abc.js", { method: "POST" }),
    ).toBeNull();
    expect(await request(h, "/favicon.ico")).toBeNull();
  });

  test("stashes Web Share Target files and redirects to /import", async () => {
    const h = harness(precache);
    const form = new FormData();
    form.append(
      "bundle",
      new File([new Uint8Array([1, 2, 3])], "soup.crisp", {
        type: "application/crisp+cbor",
      }),
    );
    form.append("title", "ignored");
    let responded: Promise<Response> | null = null;
    h.dispatch("fetch", {
      request: new Request("https://crisp.test/import", {
        method: "POST",
        body: form,
      }),
      respondWith: (p: Promise<Response>) => {
        responded = p;
      },
    });
    const res = await (responded as Promise<Response> | null);
    expect(res?.status).toBe(303);
    expect(res?.headers.get("location")).toBe(
      "https://crisp.test/import?shared=1",
    );
    const inbox = h.caches.get("crisp-share-inbox");
    expect(inbox?.size).toBe(1);
    const stored = [...(inbox?.values() ?? [])][0];
    expect(stored?.headers.get("x-filename")).toBe("soup.crisp");
    expect(new Uint8Array(await (stored as Response).arrayBuffer())).toEqual(
      new Uint8Array([1, 2, 3]),
    );
  });
});
