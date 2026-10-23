import { describe, expect, test } from "bun:test";
import { plainText } from "../../components/recipe/RichText";
import { cookAnnouncement, cookProgressPercent } from "./cookView";

const steps = [
  {
    number: 1,
    section: "Dough",
    text: [[{ text: "Brown " }, { text: "the butter.", bold: true }]],
  },
  {
    number: 2,
    section: undefined,
    text: [[{ text: "Beat." }], [{ text: "Rest." }]],
  },
];

describe("cook progress", () => {
  test("mise is 0, done is 100, steps count through", () => {
    expect(
      cookProgressPercent({ phase: "mise", currentStep: 0, stepCount: 4 }),
    ).toBe(0);
    expect(
      cookProgressPercent({ phase: "steps", currentStep: 0, stepCount: 4 }),
    ).toBe(25);
    expect(
      cookProgressPercent({ phase: "steps", currentStep: 3, stepCount: 4 }),
    ).toBe(100);
    expect(
      cookProgressPercent({ phase: "done", currentStep: 0, stepCount: 4 }),
    ).toBe(100);
    expect(
      cookProgressPercent({ phase: "steps", currentStep: 0, stepCount: 0 }),
    ).toBe(0);
  });
});

describe("cook announcement", () => {
  test("announces the step number, section, and plain text", () => {
    expect(
      cookAnnouncement({ phase: "steps", currentStep: 0 }, steps, plainText),
    ).toBe("Step 1 of 2. Dough. Brown the butter.");
    expect(
      cookAnnouncement({ phase: "steps", currentStep: 1 }, steps, plainText),
    ).toBe("Step 2 of 2. Beat. Rest.");
  });

  test("announces mise, done, and nothing for a bad index", () => {
    expect(
      cookAnnouncement({ phase: "mise", currentStep: 0 }, steps, plainText),
    ).toMatch(/^Mise en place/);
    expect(
      cookAnnouncement({ phase: "done", currentStep: 0 }, steps, plainText),
    ).toBe("All done.");
    expect(
      cookAnnouncement({ phase: "steps", currentStep: 9 }, steps, plainText),
    ).toBe("");
  });
});
