import { type Dispatch, type RefObject, useRef } from "react";
import { Icon } from "../../components/recipe/Icon";
import { stepIngredientKey } from "../../components/recipe/model";
import { Paragraphs } from "../../components/recipe/RichText";
import { StepTimer } from "../../components/recipe/StepTimer";
import { BundleImage, StepBadges } from "../../components/recipe/Steps";
import { useSwipe } from "../../hooks/useSwipe";
import { Trans, useTranslation } from "../../i18n";
import type { CookAction, CookState } from "../../lib/cookSession";
import type { Bundle, RecipeView, StepView } from "../../lib/crisp";

/** The ingredients one step uses, as a checklist of what has gone in. */
export function StepIngredientChecklist({
  step,
  stepIndex,
  checked,
  onToggle,
}: {
  step: StepView;
  stepIndex: number;
  checked: ReadonlySet<number>;
  onToggle(key: number): void;
}) {
  const { t } = useTranslation("cook");
  if (step.ingredients.length === 0) return null;
  return (
    <section className="cook-step-ingredients" aria-labelledby="step-ingr">
      <h2 id="step-ingr" className="cook-subhead">
        {t("step.youNeed")}
      </h2>
      <ul className="cook-checklist">
        {step.ingredients.map((item, position) => {
          const key = stepIngredientKey(stepIndex, position);
          const isChecked = checked.has(key);
          return (
            <li key={position}>
              <label data-checked={isChecked}>
                <input
                  type="checkbox"
                  checked={isChecked}
                  onChange={() => onToggle(key)}
                />
                <span className="check-text">
                  {item.amount && (
                    <span className="ingredient__amount tabular">
                      {item.amount}
                    </span>
                  )}{" "}
                  <span className="ingredient__name">{item.name}</span>
                  {item.preparation && (
                    <span className="ingredient__prep">
                      , {item.preparation}
                    </span>
                  )}
                </span>
              </label>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/**
 * One step at a time: huge text, heat/temperature, this step's ingredients
 * and timers, and thumb-sized Back/Next. Swipe left/right also navigates.
 */
export function StepScreen({
  view,
  bundle,
  docId,
  state,
  dispatch,
  headingRef,
}: {
  view: RecipeView;
  bundle: Bundle;
  docId: string;
  state: CookState;
  dispatch: Dispatch<CookAction>;
  headingRef: RefObject<HTMLHeadingElement | null>;
}) {
  const { t } = useTranslation("cook");
  const bodyRef = useRef<HTMLDivElement>(null);
  useSwipe(bodyRef, {
    onSwipeLeft: () => dispatch({ type: "next" }),
    onSwipeRight: () => dispatch({ type: "prev" }),
  });
  const index = state.currentStep;
  const step = view.steps[index];
  if (!step) return null;
  const total = view.steps.length;
  const last = index === total - 1;
  const checked = new Set(state.checkedIngredients);
  const done = state.checkedSteps.includes(index);

  return (
    <>
      <div className="cook-body swipe-area" ref={bodyRef}>
        <div className="cook-stage" key={step.ref}>
          <div className="cook-main">
            <div className="cook-step-head">
              <h1
                ref={headingRef}
                tabIndex={-1}
                className="step-number cook-step-label"
              >
                <Trans
                  t={t}
                  i18nKey="step.heading"
                  values={{ number: step.number, total }}
                  components={{ muted: <span className="muted" /> }}
                />
                {done && (
                  <span className="cook-done-mark">
                    <Icon name="check" /> {t("step.done")}
                  </span>
                )}
              </h1>
              {step.section && <p className="cook-section">{step.section}</p>}
            </div>
            <StepBadges step={step} size="lg" />
            <div className="cook-step">
              <Paragraphs paragraphs={step.text} />
            </div>
            {step.note.length > 0 && (
              <div className="callout cook-note" role="note">
                <Paragraphs paragraphs={step.note} />
              </div>
            )}
            {step.image && (
              <BundleImage
                bundle={bundle}
                id={step.image}
                alt={t("step.imageAlt", { number: step.number })}
                className="cook-image"
              />
            )}
          </div>
          <aside className="cook-side" aria-label={t("step.side")}>
            {step.timers.length > 0 && (
              <div className="cook-timers">
                {step.timers.map((timer, i) => (
                  <StepTimer
                    key={i}
                    size="lg"
                    hotkey={i < 8 ? i + 1 : undefined}
                    target={{
                      docId,
                      title: view.title,
                      step,
                      index: i,
                      timer,
                    }}
                  />
                ))}
              </div>
            )}
            <StepIngredientChecklist
              step={step}
              stepIndex={index}
              checked={checked}
              onToggle={(key) =>
                dispatch({ type: "toggleIngredient", index: key })
              }
            />
            {step.equipment.length > 0 && (
              <p className="cook-equipment muted">
                <strong>{t("step.equipment")}</strong>{" "}
                {step.equipment.join(", ")}
              </p>
            )}
          </aside>
        </div>
      </div>
      <nav className="cook-nav" aria-label={t("nav.step")}>
        <button
          type="button"
          className="secondary"
          onClick={() => dispatch({ type: "prev" })}
          aria-keyshortcuts="ArrowLeft"
        >
          <Icon name="left" /> {t("step.back")}
        </button>
        <button
          type="button"
          onClick={() => dispatch({ type: "next" })}
          aria-keyshortcuts="ArrowRight Space"
        >
          {last ? t("step.finish") : t("step.next")}{" "}
          <Icon name={last ? "check" : "right"} />
        </button>
      </nav>
    </>
  );
}
