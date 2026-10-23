/**
 * Recipe card covers: the cover blob is read from the stored file only when
 * its card scrolls into view, turned into an object URL, and cached per
 * revision; the bundle is freed straight away so no WASM memory is held.
 */

import { useEffect, useState } from "react";
import { loadEngine } from "../../lib/crisp";
import { createLogger } from "../../lib/log";

const log = createLogger("covers");

/** What a cover is read from. */
export interface CoverSource {
  /** The document id. */
  docId: string;
  /** The stored revision (cache key). */
  revision: string;
  /** The stored file bytes. */
  bytes: Uint8Array;
  /** The cover blob id, if the recipe has one. */
  coverId: string | undefined;
}

const cache = new Map<string, Promise<string | null>>();
/** The cache key currently held for each document. */
const current = new Map<string, string>();

function keyOf(source: CoverSource): string {
  return `${source.docId}@${source.revision}#${source.coverId ?? ""}`;
}

/** Loads (once per revision) an object URL for a recipe's cover. */
export function coverUrl(source: CoverSource): Promise<string | null> {
  if (!source.coverId) return Promise.resolve(null);
  const key = keyOf(source);
  const hit = cache.get(key);
  if (hit) return hit;
  // A newer revision replaces the old cover: free its URL.
  const stale = current.get(source.docId);
  if (stale && stale !== key) {
    void cache.get(stale)?.then((url) => url && URL.revokeObjectURL(url));
    cache.delete(stale);
  }
  current.set(source.docId, key);
  const coverId = source.coverId;
  const started = performance.now();
  const promise = loadEngine()
    .then((engine) => {
      const bundle = engine.Bundle.open(source.bytes);
      try {
        const data = bundle.blob(coverId);
        if (!data) return null;
        return URL.createObjectURL(
          new Blob([data.bytes.slice().buffer], { type: data.media_type }),
        );
      } finally {
        bundle.free();
      }
    })
    .then(
      (url) => {
        log.trace("cover.loaded", {
          docId: source.docId,
          found: url !== null,
          ms: Math.round(performance.now() - started),
        });
        return url;
      },
      (error: unknown) => {
        log.warn("cover.failed", { docId: source.docId, error: String(error) });
        cache.delete(key);
        return null;
      },
    );
  cache.set(key, promise);
  return promise;
}

/**
 * The cover URL for a card, loaded once `element` comes near the viewport.
 * Returns `null` until loaded or when there is no cover.
 */
export function useLazyCover(
  element: HTMLElement | null,
  source: CoverSource,
): string | null {
  const [url, setUrl] = useState<string | null>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    if (!element || visible) return;
    if (typeof IntersectionObserver === "undefined") {
      setVisible(true);
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: "200px" },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [element, visible]);

  const { docId, revision, bytes, coverId } = source;
  useEffect(() => {
    if (!visible || !coverId) {
      setUrl(null);
      return;
    }
    let live = true;
    void coverUrl({ docId, revision, bytes, coverId }).then(
      (next) => live && setUrl(next),
    );
    return () => {
      live = false;
    };
  }, [visible, docId, revision, bytes, coverId]);

  return url;
}
