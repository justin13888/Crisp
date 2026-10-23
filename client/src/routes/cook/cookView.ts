/**
 * Pure derivations for the cook screen: the progress bar and the
 * screen-reader announcement of where the cook is.
 */

import type { CookState } from "../../lib/cookSession";
import type { StepView } from "../../lib/crisp";

/** How far through the recipe the cook is, 0–100. */
export function cookProgressPercent(
  state: Pick<CookState, "phase" | "currentStep" | "stepCount">,
): number {
  if (state.phase === "done") return 100;
  if (state.phase === "mise" || state.stepCount === 0) return 0;
  return Math.round(((state.currentStep + 1) / state.stepCount) * 100);
}

/** What a screen reader hears when the cook moves (polite live region). */
export function cookAnnouncement(
  state: Pick<CookState, "phase" | "currentStep">,
  steps: readonly Pick<StepView, "number" | "text" | "section">[],
  toText: (paragraphs: StepView["text"]) => string,
): string {
  if (state.phase === "mise") return "Mise en place. Gather your ingredients.";
  if (state.phase === "done") return "All done.";
  const step = steps[state.currentStep];
  if (!step) return "";
  const section = step.section ? ` ${step.section}.` : "";
  return `Step ${step.number} of ${steps.length}.${section} ${toText(step.text)}`;
}
