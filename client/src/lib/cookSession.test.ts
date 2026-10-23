import { describe, expect, test } from "bun:test";
import {
  type CookAction,
  type CookState,
  cookProgress,
  cookReducer,
  initCookSession,
  normalizeScale,
} from "./cookSession";

function run(state: CookState, ...actions: CookAction[]): CookState {
  return actions.reduce(cookReducer, state);
}

const next: CookAction = { type: "next" };
const prev: CookAction = { type: "prev" };

describe("initCookSession", () => {
  test("starts at mise en place with defaults", () => {
    expect(initCookSession(3)).toEqual({
      stepCount: 3,
      phase: "mise",
      currentStep: 0,
      checkedIngredients: [],
      checkedSteps: [],
      scale: { num: 1, den: 1 },
      unitSystem: "original",
    });
  });

  test("restores and sanitizes saved progress", () => {
    const s = initCookSession(3, {
      phase: "steps",
      currentStep: 9,
      checkedIngredients: [4, 1, 1, -1, 2.5],
      checkedSteps: [2, 0, 5, 0],
      scale: { num: 4, den: 2 },
      unitSystem: "metric",
    });
    expect(s.currentStep).toBe(2);
    expect(s.checkedIngredients).toEqual([1, 4]);
    expect(s.checkedSteps).toEqual([0, 2]);
    expect(s.scale).toEqual({ num: 2, den: 1 });
    expect(s.unitSystem).toBe("metric");
  });

  test("falls back on invalid step counts and scales", () => {
    const s = initCookSession(-2, { scale: { num: 0, den: 1 } });
    expect(s.stepCount).toBe(0);
    expect(s.scale).toEqual({ num: 1, den: 1 });
  });

  test("progress round-trips", () => {
    const s = run(initCookSession(4), next, next, {
      type: "toggleIngredient",
      index: 2,
    });
    expect(initCookSession(4, cookProgress(s))).toEqual(s);
  });
});

describe("navigation", () => {
  test("walks mise → steps → done, checking steps as it goes", () => {
    let s = initCookSession(3);
    s = cookReducer(s, next);
    expect([s.phase, s.currentStep]).toEqual(["steps", 0]);
    s = run(s, next, next);
    expect([s.phase, s.currentStep]).toEqual(["steps", 2]);
    expect(s.checkedSteps).toEqual([0, 1]);
    s = cookReducer(s, next);
    expect(s.phase).toBe("done");
    expect(s.checkedSteps).toEqual([0, 1, 2]);
    expect(cookReducer(s, next)).toBe(s);
  });

  test("prev walks back to mise and is bounded there", () => {
    let s = run(initCookSession(2), next, next, next);
    expect(s.phase).toBe("done");
    s = cookReducer(s, prev);
    expect([s.phase, s.currentStep]).toEqual(["steps", 1]);
    s = run(s, prev, prev);
    expect(s.phase).toBe("mise");
    expect(cookReducer(s, prev)).toBe(s);
  });

  test("a recipe without steps goes straight from mise to done", () => {
    const s = cookReducer(initCookSession(0), next);
    expect(s.phase).toBe("done");
    expect(cookReducer(s, prev).phase).toBe("mise");
    expect(cookReducer(s, { type: "goTo", step: 0 })).toBe(s);
  });

  test("goTo jumps within bounds only", () => {
    const s = initCookSession(5);
    const at3 = cookReducer(s, { type: "goTo", step: 3 });
    expect([at3.phase, at3.currentStep]).toEqual(["steps", 3]);
    expect(cookReducer(at3, { type: "goTo", step: 3 })).toBe(at3);
    for (const step of [-1, 5, 1.5, Number.NaN]) {
      expect(cookReducer(s, { type: "goTo", step })).toBe(s);
    }
  });

  test("finish and reset", () => {
    let s = run(
      initCookSession(3),
      next,
      { type: "toggleIngredient", index: 0 },
      { type: "setScale", scale: { num: 2, den: 1 } },
      { type: "setUnits", unitSystem: "us" },
      { type: "finish" },
    );
    expect(s.phase).toBe("done");
    expect(cookReducer(s, { type: "finish" })).toBe(s);
    s = cookReducer(s, { type: "reset" });
    expect(s).toEqual({
      ...initCookSession(3),
      scale: { num: 2, den: 1 },
      unitSystem: "us",
    });
  });
});

describe("checklists", () => {
  test("toggles ingredients, keeping them sorted", () => {
    let s = initCookSession(1);
    s = run(
      s,
      { type: "toggleIngredient", index: 3 },
      { type: "toggleIngredient", index: 0 },
      { type: "toggleIngredient", index: 2 },
    );
    expect(s.checkedIngredients).toEqual([0, 2, 3]);
    s = cookReducer(s, { type: "toggleIngredient", index: 2 });
    expect(s.checkedIngredients).toEqual([0, 3]);
    expect(cookReducer(s, { type: "toggleIngredient", index: -1 })).toBe(s);
  });

  test("toggles steps bounded by stepCount", () => {
    const s = initCookSession(2);
    const toggled = cookReducer(s, { type: "toggleStep", index: 1 });
    expect(toggled.checkedSteps).toEqual([1]);
    expect(
      cookReducer(toggled, { type: "toggleStep", index: 1 }).checkedSteps,
    ).toEqual([]);
    expect(cookReducer(s, { type: "toggleStep", index: 2 })).toBe(s);
  });

  test("next does not uncheck an already-checked step", () => {
    const s = run(initCookSession(3), next, {
      type: "toggleStep",
      index: 0,
    });
    expect(cookReducer(s, next).checkedSteps).toEqual([0]);
  });
});

describe("scale and units", () => {
  test("normalizes scale and rejects non-positive values", () => {
    expect(normalizeScale({ num: 6, den: 4 })).toEqual({ num: 3, den: 2 });
    expect(normalizeScale({ num: 0, den: 4 })).toBeNull();
    expect(normalizeScale({ num: 1, den: -2 })).toBeNull();
    expect(normalizeScale({ num: 1.5, den: 2 })).toBeNull();
    const s = initCookSession(1);
    expect(
      cookReducer(s, { type: "setScale", scale: { num: 0, den: 1 } }),
    ).toBe(s);
    expect(
      cookReducer(s, { type: "setScale", scale: { num: 2, den: 2 } }),
    ).toBe(s);
    expect(
      cookReducer(s, { type: "setScale", scale: { num: 1, den: 2 } }).scale,
    ).toEqual({ num: 1, den: 2 });
  });

  test("sets units, no-op when unchanged", () => {
    const s = initCookSession(1);
    expect(cookReducer(s, { type: "setUnits", unitSystem: "original" })).toBe(
      s,
    );
    expect(
      cookReducer(s, { type: "setUnits", unitSystem: "metric" }).unitSystem,
    ).toBe("metric");
  });
});
