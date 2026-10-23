import { describe, expect, test } from "bun:test";

/**
 * Guards the design tokens in index.css: every text/background pairing must
 * meet WCAG AA (4.5:1), focus rings 3:1, in both color schemes.
 */

const css = await Bun.file(new URL("./index.css", import.meta.url)).text();

/** Extracts `--color-*` hex tokens from the first block after `marker`. */
function tokens(marker: string): Map<string, string> {
  const start = css.indexOf(marker);
  if (start < 0) throw new Error(`missing ${marker}`);
  const open = css.indexOf("{", css.indexOf(":root", start));
  const close = css.indexOf("}", open);
  const block = css.slice(open, close);
  const map = new Map<string, string>();
  for (const m of block.matchAll(/--(color-[\w-]+):\s*(#[0-9a-f]{6})/gi)) {
    if (m[1] && m[2]) map.set(m[1], m[2]);
  }
  return map;
}

function luminance(hex: string): number {
  const channels = [1, 3, 5].map((i) => {
    const c = Number.parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  const [r = 0, g = 0, b = 0] = channels;
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return ((hi ?? 0) + 0.05) / ((lo ?? 0) + 0.05);
}

const TEXT_PAIRS: [string, string][] = [
  ["color-text", "color-bg"],
  ["color-text", "color-surface"],
  ["color-text", "color-surface-2"],
  ["color-text-muted", "color-bg"],
  ["color-text-muted", "color-surface"],
  ["color-text-muted", "color-surface-2"],
  ["color-accent", "color-bg"],
  ["color-accent", "color-surface"],
  ["color-accent", "color-accent-soft"],
  ["color-accent-contrast", "color-accent"],
  ["color-accent-contrast", "color-accent-hover"],
  ["color-ok", "color-ok-bg"],
  ["color-warn", "color-warn-bg"],
  ["color-danger", "color-danger-bg"],
  ["color-danger-contrast", "color-danger"],
  ["color-alert", "color-alert-bg"],
  ["color-alert", "color-surface"],
  ["color-bg", "color-text"],
];

const UI_PAIRS: [string, string][] = [
  ["color-focus", "color-bg"],
  ["color-focus", "color-surface"],
  ["color-focus", "color-surface-2"],
  ["color-border-strong", "color-surface"],
];

for (const [scheme, marker] of [
  ["light", ":root {"],
  ["dark", "prefers-color-scheme: dark"],
] as const) {
  describe(`${scheme} palette`, () => {
    const map = tokens(marker);
    const get = (name: string): string => {
      const value = map.get(name);
      if (!value) throw new Error(`${scheme}: missing --${name}`);
      return value;
    };

    for (const [fg, bg] of TEXT_PAIRS) {
      test(`${fg} on ${bg} is AA (>= 4.5:1)`, () => {
        expect(contrast(get(fg), get(bg))).toBeGreaterThanOrEqual(4.5);
      });
    }
    for (const [fg, bg] of UI_PAIRS) {
      test(`${fg} on ${bg} is >= 3:1`, () => {
        expect(contrast(get(fg), get(bg))).toBeGreaterThanOrEqual(3);
      });
    }
  });
}

describe("touch targets", () => {
  test("touch tokens are at least 48px", () => {
    for (const name of ["touch", "touch-lg", "touch-xl"]) {
      const m = css.match(new RegExp(`--${name}:\\s*(\\d+)px`));
      expect(Number(m?.[1])).toBeGreaterThanOrEqual(48);
    }
  });

  test("cook mode body text is at least 20px", () => {
    const m = css.match(/--cook-body:\s*([\d.]+)rem/);
    expect(Number(m?.[1]) * 16).toBeGreaterThanOrEqual(20);
  });
});
