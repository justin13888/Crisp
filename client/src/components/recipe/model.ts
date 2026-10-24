/**
 * Pure helpers shared by the recipe page and cook mode: serving scales,
 * timer identity, checklist keys, trust labels, and the mapping between a
 * cook session and its stored record.
 *
 * Amounts, times, and temperatures are formatted by the engine; nothing here
 * does unit or quantity math. The only arithmetic is on the serving
 * multiplier itself (a small rational) and on timer durations in seconds.
 */

import {
  type CookProgress,
  type CookState,
  normalizeScale,
  type Scale,
  UNIT_SCALE,
  type UnitSystem,
} from "../../lib/cookSession";
import type {
  CheckResult,
  Rational,
  RecipeTimerView,
  Trust,
  ViewOptions,
} from "../../lib/crisp";
import type { CookRecord } from "../../lib/store";

// ------------------------------------------------------------------ scale

/** A one-tap serving multiplier. */
export interface ScalePreset {
  /** Visible label, e.g. `"½×"`. */
  label: string;
  /** The multiplier. */
  scale: Scale;
}

/** The ½× 1× 2× 3× presets. */
export const SCALE_PRESETS: readonly ScalePreset[] = [
  { label: "½×", scale: { num: 1, den: 2 } },
  { label: "1×", scale: { num: 1, den: 1 } },
  { label: "2×", scale: { num: 2, den: 1 } },
  { label: "3×", scale: { num: 3, den: 1 } },
];

/** The multipliers the ± stepper walks through, smallest first. */
export const SCALE_STEPS: readonly Scale[] = [
  { num: 1, den: 4 },
  { num: 1, den: 3 },
  { num: 1, den: 2 },
  { num: 2, den: 3 },
  { num: 3, den: 4 },
  { num: 1, den: 1 },
  { num: 3, den: 2 },
  { num: 2, den: 1 },
  { num: 5, den: 2 },
  { num: 3, den: 1 },
  { num: 4, den: 1 },
  { num: 5, den: 1 },
  { num: 6, den: 1 },
  { num: 8, den: 1 },
  { num: 10, den: 1 },
];

/** Largest denominator accepted when parsing a decimal serving count. */
const MAX_DECIMALS = 3;

/** Whether two scales are the same multiplier. */
export function sameScale(a: Scale, b: Scale): boolean {
  return a.num * b.den === b.num * a.den;
}

/**
 * The next (`+1`) or previous (`-1`) stepper multiplier from `current`, or
 * `null` at either end. Off-list values step to the nearest listed neighbour.
 */
export function stepScale(current: Scale, direction: 1 | -1): Scale | null {
  const value = current.num / current.den;
  if (direction > 0) {
    return SCALE_STEPS.find((s) => s.num / s.den > value + 1e-9) ?? null;
  }
  return (
    [...SCALE_STEPS].reverse().find((s) => s.num / s.den < value - 1e-9) ?? null
  );
}

/**
 * Parses a positive decimal such as `"36"`, `"1.5"`, or `"2,5"` into an exact
 * rational, or `null`.
 */
export function parseDecimal(text: string): Scale | null {
  const match = /^\s*(\d{1,6})(?:[.,](\d{1,3}))?\s*$/.exec(text);
  if (!match) return null;
  const whole = match[1] ?? "0";
  const fraction = (match[2] ?? "").slice(0, MAX_DECIMALS);
  const den = 10 ** fraction.length;
  const num = Number(whole) * den + (fraction ? Number(fraction) : 0);
  return normalizeScale({ num, den });
}

/**
 * The scale that makes `servings` of the recipe's yield: `N / yield`, as an
 * exact rational. `null` when the input or the yield is unusable.
 */
export function scaleForServings(
  servings: string,
  yieldAmount: Rational | null | undefined,
): Scale | null {
  const wanted = parseDecimal(servings);
  if (!wanted || !yieldAmount) return null;
  const [yNum, yDen] = yieldAmount;
  if (!(yNum > 0 && yDen > 0)) return null;
  return normalizeScale({
    num: wanted.num * yDen,
    den: wanted.den * yNum,
  });
}

/**
 * The unscaled yield, recovered from the engine's scaled yield and the scale
 * it was rendered at (both exact rationals, so nothing is rounded).
 */
export function baseYield(
  scaledYield: Rational | null | undefined,
  scale: Scale,
): Rational | null {
  if (!scaledYield) return null;
  return [scaledYield[0] * scale.den, scaledYield[1] * scale.num];
}

const VULGAR: Record<string, string> = {
  "1/2": "½",
  "1/3": "⅓",
  "2/3": "⅔",
  "1/4": "¼",
  "3/4": "¾",
  "1/8": "⅛",
};

/** A compact multiplier label: `"1×"`, `"½×"`, `"1½×"`, `"5/7×"`. */
export function formatScale(scale: Scale): string {
  const whole = Math.floor(scale.num / scale.den);
  const rest = scale.num % scale.den;
  if (rest === 0) return `${whole}×`;
  const fraction = `${rest}/${scale.den}`;
  const glyph = VULGAR[fraction];
  if (glyph) return `${whole > 0 ? whole : ""}${glyph}×`;
  if (scale.den > 12) return `${(scale.num / scale.den).toFixed(2)}×`;
  return whole > 0 ? `${whole} ${fraction}×` : `${fraction}×`;
}

/** The engine's view options for a session's scale and units. */
export function viewOptions(
  scale: Scale,
  units: UnitSystem,
  languages: string[],
): ViewOptions {
  return { languages, scale: [scale.num, scale.den], units };
}

/** The unit-system toggle options. */
export const UNIT_OPTIONS: readonly { value: UnitSystem; label: string }[] = [
  { value: "original", label: "Original" },
  { value: "metric", label: "Metric" },
  { value: "us", label: "US" },
];

// ----------------------------------------------------------------- timers

/** The stable id of a recipe step's timer (shared by every screen). */
export function stepTimerId(
  docId: string,
  stepRef: number,
  timerIndex: number,
): string {
  return `${docId}#${stepRef}#${timerIndex}`;
}

/** How long a step timer runs: the end of a range, else its duration. */
export function timerDurationMs(timer: RecipeTimerView): number {
  return (timer.max_seconds ?? timer.seconds) * 1000;
}

/** `"45 s"`, `"5 min"`, `"1 h 5 min"`. */
export function formatShortDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h === 0 && m === 0) return `${s} s`;
  const parts: string[] = [];
  if (h > 0) parts.push(`${h} h`);
  if (m > 0) parts.push(`${m} min`);
  if (s > 0 && h === 0) parts.push(`${s} s`);
  return parts.join(" ");
}

/**
 * For a ranged timer, when to start checking: `"Check at 5 min · done by
 * 7 min"`. `null` for a single duration.
 */
export function timerRangeHint(timer: RecipeTimerView): string | null {
  const max = timer.max_seconds;
  if (max === null || max === undefined || max <= timer.seconds) return null;
  return `Check at ${formatShortDuration(timer.seconds)} · done by ${formatShortDuration(max)}`;
}

/** The label a step timer carries into the timer dock. */
export function timerLabel(timer: RecipeTimerView, stepNumber: number): string {
  const name = timer.label.trim() || `Step ${stepNumber}`;
  return `${name.charAt(0).toUpperCase()}${name.slice(1)}`;
}

// -------------------------------------------------------------- checklist

/**
 * Checklist keys share the session's `checkedIngredients` list: mise-en-place
 * rows use the ingredient's `ref`; a step's ingredient rows live in a
 * separate key space so checking "added" in a step does not un-gather it.
 */
export const STEP_KEY_BASE = 1_000_000;
/** Key-space width reserved per step. */
export const STEP_KEY_STRIDE = 1_000;

/** The checklist key of an ingredient on the mise-en-place screen. */
export function miseKey(ingredientRef: number): number {
  return ingredientRef;
}

/** The checklist key of the `position`th ingredient of step `stepIndex`. */
export function stepIngredientKey(stepIndex: number, position: number): number {
  return STEP_KEY_BASE + stepIndex * STEP_KEY_STRIDE + position;
}

/** How many of `refs` are checked on the mise-en-place screen. */
export function countGathered(
  refs: readonly number[],
  checked: readonly number[],
): number {
  const set = new Set(checked);
  return refs.filter((ref) => set.has(miseKey(ref))).length;
}

// ------------------------------------------------------------------ trust

/** How a trust level is shown, on the recipe page and in the library. */
export interface TrustInfo {
  /** Badge text on the recipe page. */
  label: string;
  /** Compact badge text for library cards. */
  shortLabel: string;
  /** Badge tone on the recipe page (`ok`, `warn`, or none). */
  tone: "ok" | "warn" | "";
  /** Badge tone on library cards, where only endorsement reads as `ok`. */
  shortTone: "ok" | "warn" | "";
  /** One-line explanation. */
  description: string;
}

/** The badge for a trust level. */
export function trustInfo(trust: Trust): TrustInfo {
  switch (trust) {
    case "endorsed":
      return {
        label: "Endorsed",
        shortLabel: "Endorsed",
        tone: "ok",
        shortTone: "ok",
        description: "Signed by the author and endorsed by a trusted server.",
      };
    case "author-signed":
      return {
        label: "Signed by author",
        shortLabel: "Signed",
        tone: "ok",
        shortTone: "",
        description: "The author's signature verifies; no server endorsement.",
      };
    case "unsigned":
      return {
        label: "Unsigned",
        shortLabel: "Unsigned",
        tone: "warn",
        shortTone: "warn",
        description: "Nobody has signed this file; its origin is unverified.",
      };
  }
}

/** The checks that failed, in report order. */
export function failedChecks(checks: readonly CheckResult[]): CheckResult[] {
  return checks.filter((check) => check.status === "fail");
}

// ------------------------------------------------------------ persistence

/** The stored record for a session. */
export function toCookRecord(
  docId: string,
  revision: string,
  state: CookState,
  now: number,
): Omit<CookRecord, "key"> {
  return {
    docId,
    revision,
    phase: state.phase,
    currentStep: state.currentStep,
    checkedIngredients: state.checkedIngredients.slice(),
    checkedSteps: state.checkedSteps.slice(),
    scale: { ...state.scale },
    unitSystem: state.unitSystem,
    updatedAt: now,
  };
}

/** The progress to restore from a stored record (if any). */
export function fromCookRecord(record: CookRecord | undefined): CookProgress {
  if (!record) return {};
  return {
    ...(record.phase ? { phase: record.phase } : {}),
    currentStep: record.currentStep,
    checkedIngredients: record.checkedIngredients,
    checkedSteps: record.checkedSteps,
    scale: normalizeScale(record.scale) ?? UNIT_SCALE,
    unitSystem: record.unitSystem,
  };
}

/** What the recipe page's main action should say for saved progress. */
export type CookResume =
  | { kind: "start" }
  | { kind: "resume"; step: number }
  | { kind: "mise" }
  | { kind: "again" };

/** Classifies saved progress for the "Start cooking" button. */
export function cookResume(state: CookState): CookResume {
  if (state.phase === "steps") {
    return { kind: "resume", step: state.currentStep + 1 };
  }
  if (state.phase === "done") return { kind: "again" };
  return state.checkedIngredients.length > 0
    ? { kind: "mise" }
    : { kind: "start" };
}
