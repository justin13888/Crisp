/**
 * Recipe photos: downscaling a picked image in a canvas (WebP, at most
 * 1600px on the long side, about 400 KB) and wrapping it as a
 * content-addressed blob with the engine.
 */

import { useEffect, useState } from "react";
import type { Blob as CrispBlob, ImageBlob } from "../../generated/crisp";
import type { Engine } from "../../lib/crisp";
import { createLogger } from "../../lib/log";

const log = createLogger("editor.images");

/** The longest side of a stored photo, in pixels. */
export const MAX_IMAGE_SIDE = 1600;
/** The size a stored photo aims to stay under. */
export const TARGET_IMAGE_BYTES = 400 * 1024;

const QUALITIES = [0.85, 0.75, 0.65, 0.55] as const;

/** Scales `width`×`height` to fit `max` on the long side (never upscales). */
export function fitWithin(
  width: number,
  height: number,
  max: number,
): { width: number; height: number } {
  const scale = Math.min(1, max / Math.max(width, height, 1));
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}

function encode(
  canvas: HTMLCanvasElement,
  type: string,
  quality: number,
): Promise<Blob | null> {
  return new Promise((resolve) => canvas.toBlob(resolve, type, quality));
}

/**
 * Downscales and re-encodes an image file, then wraps it as a blob.
 * Falls back to JPEG where the browser cannot encode WebP.
 */
export async function prepareImage(
  file: Blob,
  engine: Pick<Engine, "imageBlob">,
): Promise<ImageBlob> {
  const started = performance.now();
  const bitmap = await createImageBitmap(file);
  let { width, height } = fitWithin(
    bitmap.width,
    bitmap.height,
    MAX_IMAGE_SIDE,
  );
  let encoded: Blob | null = null;
  try {
    for (let attempt = 0; attempt < 4; attempt++) {
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext("2d");
      if (!context) throw new Error("This browser cannot process images.");
      context.drawImage(bitmap, 0, 0, width, height);
      for (const quality of QUALITIES) {
        encoded = await encode(canvas, "image/webp", quality);
        if (encoded && encoded.type !== "image/webp") {
          encoded = await encode(canvas, "image/jpeg", quality);
        }
        if (encoded && encoded.size <= TARGET_IMAGE_BYTES) break;
      }
      if (encoded && encoded.size <= TARGET_IMAGE_BYTES) break;
      ({ width, height } = fitWithin(
        width,
        height,
        Math.round(Math.max(width, height) * 0.8),
      ));
    }
  } finally {
    bitmap.close();
  }
  if (!encoded) throw new Error("Could not encode the image.");
  const bytes = new Uint8Array(await encoded.arrayBuffer());
  const image = engine.imageBlob(bytes, encoded.type, width, height);
  log.info("image.prepared", {
    id: image.id,
    type: encoded.type,
    width,
    height,
    bytes: bytes.length,
    sourceBytes: file.size,
    ms: Math.round(performance.now() - started),
  });
  return image;
}

/** An object URL for a draft blob, revoked when it changes or unmounts. */
export function useDraftBlobUrl(blob: CrispBlob | undefined): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!blob) {
      setUrl(null);
      return;
    }
    const next = URL.createObjectURL(
      new Blob([blob.bytes.slice().buffer], { type: blob.media_type }),
    );
    setUrl(next);
    return () => URL.revokeObjectURL(next);
  }, [blob]);
  return url;
}
