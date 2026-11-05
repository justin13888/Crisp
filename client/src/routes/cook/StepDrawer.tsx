import { useEffect, useRef, useState } from "react";
import { Icon } from "../../components/recipe/Icon";
import { plainText } from "../../components/recipe/RichText";
import { useTranslation } from "../../i18n";
import type { CookState } from "../../lib/cookSession";
import type { StepView } from "../../lib/crisp";

/** What the drawer can ask the session to do. */
export interface StepDrawerActions {
  /** Jump to step `index`. */
  goTo(index: number): void;
  /** Back to the mise-en-place checklist. */
  mise(): void;
  /** Mark the recipe finished. */
  finish(): void;
  /** Clear all progress. */
  restart(): void;
  /** Close the drawer. */
  close(): void;
}

/**
 * The step overview: every step with its done state, to jump anywhere, plus
 * restart and finish. A modal `<dialog>` (focus-trapped, Esc closes).
 */
export function StepDrawer({
  steps,
  state,
  actions,
}: {
  steps: readonly StepView[];
  state: CookState;
  actions: StepDrawerActions;
}) {
  const { t } = useTranslation("cook");
  const ref = useRef<HTMLDialogElement>(null);
  const [confirming, setConfirming] = useState(false);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (typeof dialog.showModal === "function" && !dialog.open) {
      dialog.showModal();
    }
    const current = dialog.querySelector<HTMLElement>('[aria-current="step"]');
    current?.focus();
    current?.scrollIntoView({ block: "center" });
  }, []);

  const inSteps = state.phase === "steps";
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: backdrop clicks only; Esc closes the dialog natively
    <dialog
      ref={ref}
      className="step-drawer"
      aria-labelledby="step-drawer-title"
      onClose={actions.close}
      onClick={(event) => {
        // A click on the backdrop lands on the dialog element itself.
        if (event.target === ref.current) actions.close();
      }}
    >
      <div className="step-drawer__panel">
        <header className="spread step-drawer__head">
          <h2 id="step-drawer-title">{t("drawer.title")}</h2>
          <button
            type="button"
            className="ghost icon"
            onClick={actions.close}
            aria-label={t("drawer.close")}
          >
            <Icon name="close" />
          </button>
        </header>
        <ol className="step-drawer__list clean">
          <li>
            <button
              type="button"
              className="step-drawer__item"
              aria-current={state.phase === "mise" ? "step" : undefined}
              onClick={actions.mise}
            >
              <span className="step-drawer__num" aria-hidden="true">
                <Icon name="list" />
              </span>
              <span className="step-drawer__text">
                <strong>{t("drawer.mise")}</strong>
              </span>
            </button>
          </li>
          {steps.map((step, i) => {
            const done = state.checkedSteps.includes(i);
            const current = inSteps && state.currentStep === i;
            return (
              <li key={step.ref}>
                <button
                  type="button"
                  className="step-drawer__item"
                  data-done={done}
                  aria-current={current ? "step" : undefined}
                  onClick={() => actions.goTo(i)}
                >
                  <span className="step-drawer__num" aria-hidden="true">
                    {done ? <Icon name="check" /> : step.number}
                  </span>
                  <span className="step-drawer__text">
                    <span className="visually-hidden">
                      {t(done ? "drawer.stepDone" : "drawer.step", {
                        number: step.number,
                      })}{" "}
                    </span>
                    {step.section && (
                      <span className="step-drawer__section">
                        {step.section}
                      </span>
                    )}
                    <span className="step-drawer__summary">
                      {plainText(step.text)}
                    </span>
                  </span>
                </button>
              </li>
            );
          })}
        </ol>
        <footer className="step-drawer__foot cluster">
          {confirming ? (
            <>
              <button
                type="button"
                className="danger"
                onClick={actions.restart}
              >
                {t("drawer.confirmRestart")}
              </button>
              <button
                type="button"
                className="secondary"
                onClick={() => setConfirming(false)}
              >
                {t("drawer.keepGoing")}
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                className="secondary"
                onClick={() => setConfirming(true)}
              >
                <Icon name="restart" /> {t("drawer.restart")}
              </button>
              <button type="button" className="ghost" onClick={actions.finish}>
                {t("drawer.finish")}
              </button>
            </>
          )}
        </footer>
      </div>
    </dialog>
  );
}
