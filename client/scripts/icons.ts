/**
 * Rasterizes `public/icon.svg` into the PWA's PNG icons:
 *   icon-192.png, icon-512.png       — `purpose: any`
 *   icon-maskable-512.png            — full-bleed background, artwork inside
 *                                      the 80% maskable safe zone
 *
 * Run with `bun run icons`; the PNGs are committed.
 */

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { Resvg } from "@resvg/resvg-js";

const publicDir = join(import.meta.dir, "..", "public");
const source = readFileSync(join(publicDir, "icon.svg"), "utf8");

/** The brand background behind the maskable artwork. */
const MASKABLE_BG = "#b5651d";
/** Maskable icons must keep content within the central 80% circle. */
const SAFE_ZONE = 0.8;

function render(svg: string, size: number, file: string): void {
  const png = new Resvg(svg, {
    fitTo: { mode: "width", value: size },
  })
    .render()
    .asPng();
  writeFileSync(join(publicDir, file), png);
  console.info(`wrote public/${file} (${size}x${size}, ${png.byteLength} B)`);
}

/** Nests the original icon, scaled into the safe zone, on a full bleed. */
function maskable(svg: string): string {
  const inner = svg.replace(/^<svg[^>]*>/, "").replace(/<\/svg>\s*$/, "");
  const offset = (512 * (1 - SAFE_ZONE)) / 2;
  return [
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">',
    `<rect width="512" height="512" fill="${MASKABLE_BG}"/>`,
    `<g transform="translate(${offset} ${offset}) scale(${SAFE_ZONE})">`,
    inner,
    "</g>",
    "</svg>",
  ].join("");
}

render(source, 192, "icon-192.png");
render(source, 512, "icon-512.png");
render(maskable(source), 512, "icon-maskable-512.png");
