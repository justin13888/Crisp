import { describe, expect, test } from "bun:test";
import { formatClock, formatDoneAgo } from "./timeFormat";

describe("formatClock", () => {
  test("formats minutes and seconds", () => {
    expect(formatClock(0)).toBe("00:00");
    expect(formatClock(5_000)).toBe("00:05");
    expect(formatClock(9 * 60_000 + 7_000)).toBe("09:07");
    expect(formatClock(59 * 60_000 + 59_000)).toBe("59:59");
  });

  test("switches to h:mm:ss from one hour", () => {
    expect(formatClock(3_600_000)).toBe("1:00:00");
    expect(formatClock(3_600_000 + 65_000)).toBe("1:01:05");
    expect(formatClock(10 * 3_600_000)).toBe("10:00:00");
  });

  test("rounds up partial seconds and clamps negatives", () => {
    expect(formatClock(1)).toBe("00:01");
    expect(formatClock(59_001)).toBe("01:00");
    expect(formatClock(-5_000)).toBe("00:00");
  });
});

describe("formatDoneAgo", () => {
  test("describes elapsed time since finishing", () => {
    expect(formatDoneAgo(0)).toBe("Done just now");
    expect(formatDoneAgo(59_999)).toBe("Done just now");
    expect(formatDoneAgo(60_000)).toBe("Done 1 min ago");
    expect(formatDoneAgo(45 * 60_000)).toBe("Done 45 min ago");
    expect(formatDoneAgo(120 * 60_000)).toBe("Done 2 h ago");
    expect(formatDoneAgo(125 * 60_000)).toBe("Done 2 h 5 min ago");
  });
});
