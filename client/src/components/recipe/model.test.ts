import { describe, expect, test } from "bun:test";
import { cookReducer, initCookSession } from "../../lib/cookSession";
import type {
  ComplianceReport,
  NumberStyle,
  ParsedAmount,
  Quantity,
  Rational,
  RecipeTimerView,
} from "../../lib/crisp";
import {
  cookResume,
  countGathered,
  durationText,
  failedChecks,
  formatScale,
  fromCookRecord,
  miseKey,
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

/**
 * A stand-in for the engine: whole numbers and one-decimal numbers parse
 * plainly; a few fixed inputs carry a unit, a range, or `~`; anything else
 * is text. Formatting echoes its input so tests see what was asked for.
 */
const engine = {
  parseAmount(text: string): ParsedAmount {
    const typed = text.trim();
    if (!typed) throw new Error("the amount is empty");
    const fixed: Record<string, Quantity> = {
      "1 cup": { min: [1, 1], unit: "[cup_us]" },
      "2-3": { min: [2, 1], max: [3, 1] },
      "~4": { min: [4, 1], approx: true },
    };
    const match = /^(\d+)(?:\.(\d))?$/.exec(typed);
    const quantity: Quantity =
      fixed[typed] ??
      (match
        ? { min: match[2] ? [Number(typed) * 10, 10] : [Number(typed), 1] }
        : { text: { en: typed } });
    return { quantity, unit_known: false };
  },
  formatNumber: ([n, d]: Rational, style: NumberStyle) => `${n}/${d} ${style}`,
  formatQuantity: (q: Quantity, lang: string) =>
    `${q.min?.join("/")} ${q.unit} ${lang}`,
};

/** `factorForYield` for a recipe whose unscaled yield is `base`. */
function factorFor(base: Rational) {
  const targets: Rational[] = [];
  const factor = (target: Rational): Rational | undefined => {
    targets.push(target);
    return base[0] > 0 ? [target[0] * base[1], target[1] * base[0]] : undefined;
  };
  return { factor, targets };
}

describe("serving scale", () => {
  test("make N asks the engine for N / yield, reduced", () => {
    const yield24 = factorFor([24, 1]);
    expect(scaleForServings("36", "en", engine, yield24.factor)).toEqual({
      num: 3,
      den: 2,
    });
    expect(yield24.targets).toEqual([[36, 1]]);
    const yield6 = factorFor([6, 1]);
    expect(scaleForServings("1.5", "en", engine, yield6.factor)).toEqual({
      num: 1,
      den: 4,
    });
  });

  test("make N needs a plain positive number and a yield", () => {
    const yield4 = factorFor([4, 1]);
    for (const bad of ["", "x", "1 cup", "2-3", "~4", "0"]) {
      expect(scaleForServings(bad, "en", engine, yield4.factor)).toBeNull();
    }
    expect(yield4.targets).toEqual([]);
    const none = factorFor([0, 1]).factor;
    expect(scaleForServings("4", "en", engine, none)).toBeNull();
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
    expect(formatScale({ num: 3, den: 2 }, engine)).toBe("3/2 fraction×");
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
    const minutes = (seconds: number) => `${seconds / 60} min`;
    expect(timerRangeHint(timer(), minutes)).toBeNull();
    expect(
      timerRangeHint(timer({ seconds: 300, max_seconds: 420 }), minutes),
    ).toBe("Check at 5 min · done by 7 min");
    expect(timerRangeHint(timer({ max_seconds: 600 }), minutes)).toBeNull();
  });

  test("durations are written by the engine as seconds", () => {
    expect(durationText(300, engine, "fr")).toBe("300/1 s fr");
  });

  test("labels are capitalized", () => {
    expect(timerLabel(timer({ label: "brown butter" }))).toBe("Brown butter");
    expect(timerLabel(timer({ label: "Step 3" }))).toBe("Step 3");
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

  test("has compact labels and tones for library cards", () => {
    expect(trustInfo("endorsed")).toMatchObject({
      shortLabel: "Endorsed",
      shortTone: "ok",
    });
    expect(trustInfo("author-signed")).toMatchObject({
      shortLabel: "Signed",
      shortTone: "",
    });
    expect(trustInfo("unsigned")).toMatchObject({
      shortLabel: "Unsigned",
      shortTone: "warn",
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
    expect(failedChecks(report.checks).map((c) => c.id)).toEqual(["D2"]);
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
