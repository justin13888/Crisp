import { describe, expect, test } from "bun:test";
import { fitWithin, MAX_IMAGE_SIDE } from "./images";

describe("fitWithin", () => {
  test("scales the long side down to the limit, keeping the aspect", () => {
    expect(fitWithin(4000, 3000, MAX_IMAGE_SIDE)).toEqual({
      width: 1600,
      height: 1200,
    });
    expect(fitWithin(1000, 3000, 1500)).toEqual({ width: 500, height: 1500 });
  });

  test("never upscales and never returns zero", () => {
    expect(fitWithin(800, 600, MAX_IMAGE_SIDE)).toEqual({
      width: 800,
      height: 600,
    });
    expect(fitWithin(10_000, 1, 100)).toEqual({ width: 100, height: 1 });
  });
});
