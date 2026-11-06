/**
 * Pure derivations for the cook screen: the progress bar and the
 * screen-reader announcement of where the cook is.
 */

import { i18n } from "../../i18n";
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
  if (state.phase === "mise") return i18n.t("cook:announce.mise");
  if (state.phase === "done") return i18n.t("cook:announce.done");
  const step = steps[state.currentStep];
  if (!step) return "";
  const values = {
    number: step.number,
    total: steps.length,
    text: toText(step.text),
  };
  return step.section
    ? i18n.t("cook:announce.sectionStep", { ...values, section: step.section })
    : i18n.t("cook:announce.step", values);
}
