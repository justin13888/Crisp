import { describe, expect, test } from "bun:test";
import { cookReducer, initCookSession } from "../../lib/cookSession";
import type { ComplianceReport, RecipeTimerView } from "../../lib/crisp";
import {
  baseYield,
  cookResume,
  countGathered,
  failedChecks,
  formatScale,
  formatShortDuration,
  fromCookRecord,
  miseKey,
  parseDecimal,
  SCALE_PRESETS,
  sameScale,
  scaleForServings,
  STEP_KEY_BASE,
  stepIngredientKey,
  stepScale,
  stepTimerId,
  timerDurationMs,
  timerLabel,
  timerRangeHint,
  toCookRecord,
  trustInfo,
  viewOptions,
} from "./model";

function timer(over: Partial<RecipeTimerView> = {}): RecipeTimerView {
  return {
    label: "bake",
    seconds: 600,
    max_seconds: undefined,
    text: "10 min",
    passive: false,
    ...over,
  };
}

describe("serving scale", () => {
  test("parses decimals exactly", () => {
    expect(parseDecimal("36")).toEqual({ num: 36, den: 1 });
    expect(parseDecimal("1.5")).toEqual({ num: 3, den: 2 });
    expect(parseDecimal("2,25")).toEqual({ num: 9, den: 4 });
    expect(parseDecimal(" 12 ")).toEqual({ num: 12, den: 1 });
  });

  test("rejects junk, zero, and negatives", () => {
    for (const bad of ["", "abc", "0", "-2", "1/2", "1e3", "0.0"]) {
      expect(parseDecimal(bad)).toBeNull();
    }
  });

  test("make N of the yield is N / yield, reduced", () => {
    expect(scaleForServings("36", [24, 1])).toEqual({ num: 3, den: 2 });
    expect(scaleForServings("12", [24, 1])).toEqual({ num: 1, den: 2 });
    expect(scaleForServings("4", [4, 1])).toEqual({ num: 1, den: 1 });
    expect(scaleForServings("5", [3, 2])).toEqual({ num: 10, den: 3 });
    expect(scaleForServings("1.5", [6, 1])).toEqual({ num: 1, den: 4 });
  });

  test("recovers the unscaled yield exactly", () => {
    expect(baseYield([48, 1], { num: 2, den: 1 })).toEqual([48, 2]);
    expect(baseYield([36, 1], { num: 3, den: 2 })).toEqual([72, 3]);
    expect(baseYield(null, { num: 2, den: 1 })).toBeNull();
    // Make 24 at 2x: back to 1x.
    expect(
      scaleForServings("24", baseYield([48, 1], { num: 2, den: 1 })),
    ).toEqual({ num: 1, den: 1 });
  });

  test("make N needs a usable yield and input", () => {
    expect(scaleForServings("4", null)).toBeNull();
    expect(scaleForServings("4", undefined)).toBeNull();
    expect(scaleForServings("4", [0, 1])).toBeNull();
    expect(scaleForServings("x", [4, 1])).toBeNull();
  });

  test("stepper walks the ladder and stops at the ends", () => {
    expect(stepScale({ num: 1, den: 1 }, 1)).toEqual({ num: 3, den: 2 });
    expect(stepScale({ num: 1, den: 1 }, -1)).toEqual({ num: 3, den: 4 });
    expect(stepScale({ num: 10, den: 1 }, 1)).toBeNull();
    expect(stepScale({ num: 1, den: 4 }, -1)).toBeNull();
    // Off-ladder values step to the neighbour.
    expect(stepScale({ num: 7, den: 5 }, 1)).toEqual({ num: 3, den: 2 });
    expect(stepScale({ num: 7, den: 5 }, -1)).toEqual({ num: 1, den: 1 });
  });

  test("compares and formats multipliers", () => {
    expect(sameScale({ num: 2, den: 4 }, { num: 1, den: 2 })).toBe(true);
    expect(sameScale({ num: 1, den: 1 }, { num: 2, den: 1 })).toBe(false);
    expect(formatScale({ num: 1, den: 1 })).toBe("1×");
    expect(formatScale({ num: 1, den: 2 })).toBe("½×");
    expect(formatScale({ num: 3, den: 2 })).toBe("1½×");
    expect(formatScale({ num: 10, den: 3 })).toBe("3⅓×");
    expect(formatScale({ num: 5, den: 7 })).toBe("5/7×");
    expect(formatScale({ num: 7, den: 5 })).toBe("1 2/5×");
    expect(formatScale({ num: 50, den: 37 })).toBe("1.35×");
    expect(SCALE_PRESETS.map((p) => p.label)).toEqual(["½×", "1×", "2×", "3×"]);
  });

  test("builds engine view options", () => {
    expect(viewOptions({ num: 3, den: 2 }, "metric", ["en"])).toEqual({
      languages: ["en"],
      scale: [3, 2],
      units: "metric",
    });
  });
});

describe("timers", () => {
  test("ids are stable per doc, step, and timer", () => {
    expect(stepTimerId("rcp1abc", 4, 0)).toBe("rcp1abc#4#0");
    expect(stepTimerId("rcp1abc", 4, 1)).not.toBe(stepTimerId("rcp1abc", 4, 0));
  });

  test("ranges run to their maximum", () => {
    expect(timerDurationMs(timer())).toBe(600_000);
    expect(timerDurationMs(timer({ max_seconds: 720 }))).toBe(720_000);
  });

  test("ranges get a check-at hint", () => {
    expect(timerRangeHint(timer())).toBeNull();
    expect(timerRangeHint(timer({ seconds: 300, max_seconds: 420 }))).toBe(
      "Check at 5 min · done by 7 min",
    );
    expect(timerRangeHint(timer({ max_seconds: 600 }))).toBeNull();
  });

  test("short durations", () => {
    expect(formatShortDuration(45)).toBe("45 s");
    expect(formatShortDuration(90)).toBe("1 min 30 s");
    expect(formatShortDuration(300)).toBe("5 min");
    expect(formatShortDuration(3900)).toBe("1 h 5 min");
    expect(formatShortDuration(7200)).toBe("2 h");
  });

  test("labels are capitalized, falling back to the step", () => {
    expect(timerLabel(timer({ label: "brown butter" }), 1)).toBe(
      "Brown butter",
    );
    expect(timerLabel(timer({ label: "  " }), 3)).toBe("Step 3");
  });
});

describe("checklists", () => {
  test("mise and step keys never collide", () => {
    expect(miseKey(7)).toBe(7);
    expect(stepIngredientKey(0, 0)).toBe(STEP_KEY_BASE);
    expect(stepIngredientKey(2, 3)).toBe(STEP_KEY_BASE + 2003);
    const keys = new Set<number>();
    for (let step = 0; step < 50; step++) {
      for (let pos = 0; pos < 30; pos++) keys.add(stepIngredientKey(step, pos));
    }
    expect(keys.size).toBe(50 * 30);
    expect([...keys].every((k) => k >= STEP_KEY_BASE)).toBe(true);
  });

  test("counts gathered ingredients only", () => {
    expect(countGathered([0, 1, 2], [0, 2, stepIngredientKey(0, 1)])).toBe(2);
    expect(countGathered([], [1])).toBe(0);
  });
});

describe("trust", () => {
  test("labels each level", () => {
    expect(trustInfo("endorsed").label).toBe("Endorsed");
    expect(trustInfo("author-signed").label).toBe("Signed by author");
    expect(trustInfo("unsigned")).toMatchObject({
      label: "Unsigned",
      tone: "warn",
    });
  });

  test("lists failed checks", () => {
    const report: ComplianceReport = {
      crisp_report: 1,
      compliant: false,
      class: "structural",
      format_version: 1,
      trust: "unsigned",
      verified_at: 0,
      checks: [
        { id: "A1", status: "pass" },
        { id: "D2", status: "fail", code: "D.SIG", detail: "bad signature" },
        { id: "E2", status: "skipped" },
      ],
    };
    expect(failedChecks(report).map((c) => c.id)).toEqual(["D2"]);
  });
});

describe("cook records", () => {
  test("round-trip a session through the store shape", () => {
    let state = initCookSession(5);
    state = cookReducer(state, { type: "toggleIngredient", index: 3 });
    state = cookReducer(state, { type: "next" });
    state = cookReducer(state, { type: "next" });
    state = cookReducer(state, {
      type: "setScale",
      scale: { num: 2, den: 1 },
    });
    state = cookReducer(state, { type: "setUnits", unitSystem: "metric" });
    const record = toCookRecord("rcp1", "rev1", state, 42);
    expect(record).toEqual({
      docId: "rcp1",
      revision: "rev1",
      phase: "steps",
      currentStep: 1,
      checkedIngredients: [3],
      checkedSteps: [0],
      scale: { num: 2, den: 1 },
      unitSystem: "metric",
      updatedAt: 42,
    });
    const restored = initCookSession(
      5,
      fromCookRecord({ ...record, key: "rcp1@rev1" }),
    );
    expect(restored).toEqual(state);
  });

  test("no record restores nothing; a bad scale falls back to 1×", () => {
    expect(fromCookRecord(undefined)).toEqual({});
    const progress = fromCookRecord({
      key: "k",
      docId: "d",
      revision: "r",
      currentStep: 0,
      checkedIngredients: [],
      checkedSteps: [],
      scale: { num: 0, den: 1 },
      unitSystem: "original",
      updatedAt: 0,
    });
    expect(progress.scale).toEqual({ num: 1, den: 1 });
    expect(progress.phase).toBeUndefined();
  });

  test("classifies resume state", () => {
    let state = initCookSession(3);
    expect(cookResume(state)).toEqual({ kind: "start" });
    state = cookReducer(state, { type: "toggleIngredient", index: 0 });
    expect(cookResume(state)).toEqual({ kind: "mise" });
    state = cookReducer(state, { type: "goTo", step: 2 });
    expect(cookResume(state)).toEqual({ kind: "resume", step: 3 });
    state = cookReducer(state, { type: "finish" });
    expect(cookResume(state)).toEqual({ kind: "again" });
  });
});
