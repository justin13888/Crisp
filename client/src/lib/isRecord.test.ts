import { describe, expect, test } from "bun:test";
import { isRecord } from "./isRecord";

describe("isRecord", () => {
  test("accepts plain objects", () => {
    expect(isRecord({})).toBe(true);
    expect(isRecord({ a: 1 })).toBe(true);
    expect(isRecord(Object.create(null))).toBe(true);
  });

  test("refuses null, arrays, and primitives", () => {
    for (const value of [null, undefined, [], [1], "x", 1, true]) {
      expect(isRecord(value)).toBe(false);
    }
  });
});
