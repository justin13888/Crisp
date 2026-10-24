import type { Dispatch, RefObject } from "react";
import { Link } from "react-router-dom";
import { Icon } from "../../components/recipe/Icon";
import {
  EquipmentList,
  IngredientSections,
} from "../../components/recipe/Ingredients";
import {
  countGathered,
  formatScale,
  UNIT_OPTIONS,
} from "../../components/recipe/model";
import { ScaleControls } from "../../components/recipe/ScaleControls";
import { useEngine } from "../../hooks/useEngine";
import type { CookAction, CookState } from "../../lib/cookSession";
import type { Bundle, RecipeView } from "../../lib/crisp";

/** Total, active, and hands-off time at a glance. */
export function TimeSummary({ view }: { view: RecipeView }) {
  const total = view.times.find((t) => t.label.toLowerCase() === "total");
  const items: { label: string; value: string }[] = [];
  if (total) items.push({ label: "Total", value: total.value });
  if (view.active_timer_seconds > 0) {
    items.push({ label: "Active", value: view.active_timer_text });
  }
  if (view.passive_timer_seconds > 0) {
    items.push({ label: "Hands-off", value: view.passive_timer_text });
  }
  if (items.length === 0) return null;
  return (
    <dl className="time-summary">
      {items.map((item) => (
        <div key={item.label}>
          <dt>{item.label}</dt>
          <dd className="tabular">{item.value}</dd>
        </div>
      ))}
    </dl>
  );
}

/**
 * The mise-en-place phase: every scaled ingredient as a big checklist, the
 * equipment, the time budget, and the scale/units controls.
 */
export function MiseScreen({
  view,
  bundle,
  state,
  dispatch,
  headingRef,
  recipeHref,
}: {
  view: RecipeView;
  bundle: Bundle;
  state: CookState;
  dispatch: Dispatch<CookAction>;
  headingRef: RefObject<HTMLHeadingElement | null>;
  recipeHref: string;
}) {
  const { engine } = useEngine();
  const refs = view.ingredients.flatMap((s) => s.items.map((i) => i.ref));
  const checked = new Set(state.checkedIngredients);
  const gathered = countGathered(refs, state.checkedIngredients);
  const unitsLabel =
    UNIT_OPTIONS.find((o) => o.value === state.unitSystem)?.label ?? "";
  const all = refs.length > 0 && gathered === refs.length;

  return (
    <>
      <div className="cook-body">
        <div className="cook-mise">
          <div className="cook-mise__intro">
            <h1 ref={headingRef} tabIndex={-1} className="cook-title">
              Mise en place
            </h1>
            <p className="muted">
              Gather everything before you start. Tap a row to check it off.
            </p>
            <TimeSummary view={view} />
            <details className="cook-scale">
              <summary>
                <span>
                  {engine && (
                    <strong>{formatScale(state.scale, engine)}</strong>
                  )}
                  {view.yield_text && <> · {view.yield_text}</>} · {unitsLabel}{" "}
                  units
                </span>
                <span className="cook-scale__edit">Change</span>
              </summary>
              <ScaleControls
                scale={state.scale}
                unitSystem={state.unitSystem}
                yieldText={view.yield_text}
                yieldAmount={view.yield_amount}
                yieldUnit={view.yield_unit}
                language={view.language}
                factorForYield={(target) => bundle.factorForYield(0, target)}
                onScale={(scale) => dispatch({ type: "setScale", scale })}
                onUnits={(unitSystem) =>
                  dispatch({ type: "setUnits", unitSystem })
                }
              />
            </details>
          </div>
          <section className="cook-mise__list" aria-labelledby="mise-ingr">
            <div className="spread">
              <h2 id="mise-ingr">Ingredients</h2>
              <span
                className={`badge${all ? " ok" : ""} tabular`}
                aria-live="polite"
              >
                {gathered} of {refs.length} ready
              </span>
            </div>
            <IngredientSections
              sections={view.ingredients}
              headingLevel={3}
              checklist={{
                checked,
                onToggle: (ref) =>
                  dispatch({ type: "toggleIngredient", index: ref }),
              }}
            />
            {view.equipment.length > 0 && (
              <>
                <h2>Equipment</h2>
                <EquipmentList equipment={view.equipment} />
              </>
            )}
          </section>
        </div>
      </div>
      <nav className="cook-nav" aria-label="Cook navigation">
        <Link className="button secondary" to={recipeHref}>
          <Icon name="left" /> Recipe
        </Link>
        <button type="button" onClick={() => dispatch({ type: "next" })}>
          {view.steps.length === 0 ? "Finish" : "Start cooking"}{" "}
          <Icon name="right" />
        </button>
      </nav>
    </>
  );
}
