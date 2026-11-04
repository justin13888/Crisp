import { describe, expect, test } from "bun:test";
import { monthName, toggleMonth } from "./MonthsField";

describe("toggleMonth", () => {
  test("adds a month in calendar order and removes a picked one", () => {
    expect(toggleMonth([], 5)).toEqual([5]);
    expect(toggleMonth([1, 12], 6)).toEqual([1, 6, 12]);
    expect(toggleMonth([1, 6, 12], 6)).toEqual([1, 12]);
  });
});

describe("monthName", () => {
  test("names each month in the given locale", () => {
    expect(monthName(1, "en-US")).toBe("Jan");
    expect(monthName(12, "en-US")).toBe("Dec");
  });
});
