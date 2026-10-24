import { describe, expect, test } from "bun:test";
import type * as Wasm from "../../generated/crisp";
import type { ParsedTemperature } from "../../generated/crisp";
import { readDegrees, type TemperatureEngine } from "./temperature";

/** Returns `parsed` for any text, or throws when it is undefined. */
function engine(parsed?: ParsedTemperature): TemperatureEngine {
  return {
    parseTemperature(text: string): ParsedTemperature {
      if (!parsed) throw new Error(`"${text}" is not a temperature`);
      return parsed;
    },
  };
}

describe("readDegrees", () => {
  test("a bare number takes the step's unit", () => {
    expect(readDegrees(engine({ min: [180, 1] }), "180", "Cel", "en")).toEqual({
      value: [180, 1],
    });
  });

  test("a typed unit matching the step's is accepted", () => {
    const parsed: ParsedTemperature = { min: [350, 1], unit: "[degF]" };
    expect(readDegrees(engine(parsed), "350 F", "[degF]", "en")).toEqual({
      value: [350, 1],
    });
  });

  test("a typed unit disagreeing with the step's is refused", () => {
    const parsed: ParsedTemperature = { min: [350, 1], unit: "[degF]" };
    const read = readDegrees(engine(parsed), "350 F", "Cel", "en");
    expect(read).toEqual({
      problem: `"350 F" is in °F, but the step is set to °C — change the unit or the number.`,
    });
  });

  test("kelvin in a °F step is refused", () => {
    const parsed: ParsedTemperature = { min: [300, 1], unit: "K" };
    expect(readDegrees(engine(parsed), "300 K", "[degF]", "en")).toHaveProperty(
      "problem",
    );
  });

  test("a range is refused", () => {
    const parsed: ParsedTemperature = { min: [180, 1], max: [200, 1] };
    expect(readDegrees(engine(parsed), "180-200", "Cel", "en")).toEqual({
      problem: `"180-200" is not a number.`,
    });
  });

  test("text the engine rejects is refused", () => {
    expect(readDegrees(engine(), "hot", "Cel", "en")).toEqual({
      problem: `"hot" is not a number.`,
    });
  });
});

// ------------------------------------------------- real engine (optional)

const wasmDir = new URL("../../../public/crisp-wasm/", import.meta.url);
const haveWasm = await Bun.file(new URL("crisp_bg.wasm", wasmDir)).exists();

describe.skipIf(!haveWasm)("readDegrees with the real engine", () => {
  test("a typed °F or kelvin in a °C step is refused, not saved", async () => {
    const real: typeof Wasm = await import(
      new URL("crisp.js", wasmDir).pathname
    );
    real.initSync({
      module: await Bun.file(new URL("crisp_bg.wasm", wasmDir)).arrayBuffer(),
    });
    for (const typed of ["350 F", "300 kelvin"]) {
      expect(readDegrees(real, typed, "Cel", "en")).toHaveProperty("problem");
    }
    expect(readDegrees(real, "350 F", "[degF]", "en")).toEqual({
      value: [350, 1],
    });
  });
});
