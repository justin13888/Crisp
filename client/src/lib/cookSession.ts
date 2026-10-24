/**
 * Cook mode as a pure reducer.
 *
 * A session walks a recipe in three phases: `mise` (gather ingredients),
 * `steps` (one step at a time), and `done`. All indexes are 0-based and
 * bounded by the recipe's `stepCount`; invalid actions return the same state
 * object so React can skip re-rendering.
 */

/** The phase of a cook session. */
export type CookPhase = "mise" | "steps" | "done";

/** Which units quantities are displayed in. */
export type UnitSystem = "metric" | "us" | "original";

/** A positive rational scale factor (`num / den`), kept in lowest terms. */
export interface Scale {
  /** Numerator (positive integer). */
  num: number;
  /** Denominator (positive integer). */
  den: number;
}

/** The full cook-session state. */
export interface CookState {
  /** Number of steps in the recipe (the bound for step indexes). */
  stepCount: number;
  /** Current phase. */
  phase: CookPhase;
  /** The 0-based step in view (meaningful in `steps`). */
  currentStep: number;
  /** Checked ingredient indexes, sorted and unique. */
  checkedIngredients: number[];
  /** Completed step indexes, sorted and unique. */
  checkedSteps: number[];
  /** The serving scale. */
  scale: Scale;
  /** The display unit system. */
  unitSystem: UnitSystem;
}

/** Everything a cook session can do. */
export type CookAction =
  | { type: "next" }
  | { type: "prev" }
  | { type: "goTo"; step: number }
  | { type: "toggleIngredient"; index: number }
  | { type: "toggleStep"; index: number }
  | { type: "setScale"; scale: Scale }
  | { type: "setUnits"; unitSystem: UnitSystem }
  | { type: "finish" }
  | { type: "reset" };

/** The persistable subset of a session (see `store/` `cook` records). */
export type CookProgress = Partial<
  Pick<
    CookState,
    | "phase"
    | "currentStep"
    | "checkedIngredients"
    | "checkedSteps"
    | "scale"
    | "unitSystem"
  >
>;

/** The identity scale. */
export const UNIT_SCALE: Scale = { num: 1, den: 1 };

function gcd(a: number, b: number): number {
  let x = a;
  let y = b;
  while (y !== 0) [x, y] = [y, x % y];
  return x;
}

function isPositiveInt(value: number): boolean {
  return Number.isSafeInteger(value) && value > 0;
}

/** Reduces a scale to lowest terms, or `null` if it is not positive. */
export function normalizeScale(scale: Scale): Scale | null {
  if (!isPositiveInt(scale.num) || !isPositiveInt(scale.den)) return null;
  const d = gcd(scale.num, scale.den);
  return { num: scale.num / d, den: scale.den / d };
}

function isIndex(value: number, bound: number): boolean {
  return Number.isInteger(value) && value >= 0 && value < bound;
}

/** Sorted, de-duplicated, in-range indexes. */
function cleanIndexes(values: readonly number[], bound: number): number[] {
  return [...new Set(values.filter((v) => isIndex(v, bound)))].sort(
    (a, b) => a - b,
  );
}

function toggle(values: readonly number[], index: number): number[] {
  return values.includes(index)
    ? values.filter((v) => v !== index)
    : [...values, index].sort((a, b) => a - b);
}

function withChecked(values: readonly number[], index: number): number[] {
  return values.includes(index) ? values.slice() : toggle(values, index);
}

/**
 * Builds a session for a recipe with `stepCount` steps, optionally restoring
 * saved progress (out-of-range values are clamped or dropped).
 */
export function initCookSession(
  stepCount: number,
  saved: CookProgress = {},
): CookState {
  const steps = Number.isInteger(stepCount) && stepCount > 0 ? stepCount : 0;
  const lastStep = Math.max(0, steps - 1);
  const current = saved.currentStep ?? 0;
  return {
    stepCount: steps,
    phase: saved.phase ?? "mise",
    currentStep: Number.isInteger(current)
      ? Math.min(Math.max(0, current), lastStep)
      : 0,
    checkedIngredients: cleanIndexes(
      saved.checkedIngredients ?? [],
      Number.MAX_SAFE_INTEGER,
    ),
    checkedSteps: cleanIndexes(saved.checkedSteps ?? [], steps),
    scale: (saved.scale && normalizeScale(saved.scale)) ?? UNIT_SCALE,
    unitSystem: saved.unitSystem ?? "original",
  };
}

/** The cook-mode reducer. */
export function cookReducer(state: CookState, action: CookAction): CookState {
  const last = state.stepCount - 1;
  switch (action.type) {
    case "next": {
      if (state.phase === "mise") {
        return state.stepCount === 0
          ? { ...state, phase: "done" }
          : { ...state, phase: "steps", currentStep: 0 };
      }
      if (state.phase === "done") return state;
      const checkedSteps = withChecked(state.checkedSteps, state.currentStep);
      if (state.currentStep >= last) {
        return { ...state, phase: "done", checkedSteps };
      }
      return { ...state, currentStep: state.currentStep + 1, checkedSteps };
    }
    case "prev": {
      if (state.phase === "mise") return state;
      if (state.phase === "done") {
        return state.stepCount === 0
          ? { ...state, phase: "mise" }
          : { ...state, phase: "steps", currentStep: last };
      }
      if (state.currentStep === 0) return { ...state, phase: "mise" };
      return { ...state, currentStep: state.currentStep - 1 };
    }
    case "goTo": {
      if (!isIndex(action.step, state.stepCount)) return state;
      if (state.phase === "steps" && state.currentStep === action.step) {
        return state;
      }
      return { ...state, phase: "steps", currentStep: action.step };
    }
    case "toggleIngredient": {
      if (!isIndex(action.index, Number.MAX_SAFE_INTEGER)) return state;
      return {
        ...state,
        checkedIngredients: toggle(state.checkedIngredients, action.index),
      };
    }
    case "toggleStep": {
      if (!isIndex(action.index, state.stepCount)) return state;
      return {
        ...state,
        checkedSteps: toggle(state.checkedSteps, action.index),
      };
    }
    case "setScale": {
      const scale = normalizeScale(action.scale);
      if (!scale) return state;
      if (scale.num === state.scale.num && scale.den === state.scale.den) {
        return state;
      }
      return { ...state, scale };
    }
    case "setUnits":
      return action.unitSystem === state.unitSystem
        ? state
        : { ...state, unitSystem: action.unitSystem };
    case "finish":
      return state.phase === "done" ? state : { ...state, phase: "done" };
    case "reset":
      // Start over, but keep the cook's scale and unit preferences.
      return {
        ...initCookSession(state.stepCount),
        scale: state.scale,
        unitSystem: state.unitSystem,
      };
  }
}
