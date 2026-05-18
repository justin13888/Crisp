import { useEffect, useMemo, useState } from "react";
import { scaleRecipe } from "../lib/crisp";
import {
  formatQuantity,
  formatRational,
  resolveProse,
  resolveText,
} from "../lib/render";
import type { BundlePayload, Quantity, Recipe } from "../lib/types";
import { Prose } from "./Prose";
import { StepTimer } from "./StepTimer";

/** A scaling factor as an exact rational. */
interface Factor {
  num: number;
  den: number;
  label: string;
}

const FACTORS: Factor[] = [
  { num: 1, den: 2, label: "½×" },
  { num: 1, den: 1, label: "1×" },
  { num: 2, den: 1, label: "2×" },
  { num: 3, den: 1, label: "3×" },
];

/** Renders one quantity, falling back to an em dash when empty. */
function qty(
  quantity: Quantity | undefined,
  prefs: string[],
  def: string,
): string {
  return formatQuantity(quantity, prefs, def) || "—";
}

/**
 * The full recipe viewer: localized rendering, exact-rational scaling, and
 * per-step timers. A bundle must already be verified before it reaches here.
 */
export function RecipeView({
  recipe,
  payload,
}: {
  recipe: Recipe;
  payload: BundlePayload;
}) {
  const [language, setLanguage] = useState(payload.default_language);
  const [factor, setFactor] = useState<Factor>({ num: 1, den: 1, label: "1×" });
  const [shown, setShown] = useState(recipe);
  const [scaleError, setScaleError] = useState<string | null>(null);
  const prefs = useMemo(() => [language], [language]);
  const def = payload.default_language;

  useEffect(() => {
    let cancelled = false;
    if (factor.num === 1 && factor.den === 1) {
      setShown(recipe);
      setScaleError(null);
      return;
    }
    scaleRecipe(recipe, factor.num, factor.den)
      .then((scaled) => {
        if (!cancelled) {
          setShown(scaled);
          setScaleError(null);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setScaleError(error instanceof Error ? error.message : String(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [recipe, factor]);

  const ingredientName = (ref: number): string => {
    const found = shown.ingredients.find((i) => i.ref === ref);
    return found ? resolveText(found.name, prefs, def) : `#${ref}`;
  };
  const equipmentName = (ref: number): string => {
    const found = shown.equipment?.find((e) => e.ref === ref);
    return found ? resolveText(found.name, prefs, def) : `#${ref}`;
  };

  const classifications = [
    ...(recipe.cuisine ?? []),
    ...(recipe.course ?? []),
    ...(recipe.diet ?? []),
    ...(recipe.tags ?? []),
  ];

  return (
    <article>
      <h1>{resolveText(shown.title, prefs, def)}</h1>

      <div className="row" style={{ justifyContent: "space-between" }}>
        {payload.languages.length > 1 && (
          <label>
            <span>Language</span>
            <select
              value={language}
              onChange={(e) => setLanguage(e.target.value)}
            >
              {payload.languages.map((tag) => (
                <option key={tag} value={tag}>
                  {tag}
                </option>
              ))}
            </select>
          </label>
        )}
        <div className="chips">
          {FACTORS.map((f) => (
            <button
              type="button"
              key={f.label}
              className={f.label === factor.label ? "" : "secondary"}
              onClick={() => setFactor(f)}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>

      {scaleError && (
        <div className="banner bad">Could not scale: {scaleError}</div>
      )}

      {shown.summary && (
        <section>
          <Prose prose={resolveProse(shown.summary, prefs, def)} />
        </section>
      )}

      {recipe.allergens && recipe.allergens.length > 0 && (
        <p>
          <strong>Allergens:</strong> {recipe.allergens.join(", ")}
        </p>
      )}

      {classifications.length > 0 && (
        <div className="chips">
          {classifications.map((tag) => (
            <span className="chip" key={tag}>
              {tag}
            </span>
          ))}
        </div>
      )}

      <p>
        <strong>Yield:</strong> {qty(shown.yield.amount, prefs, def)}{" "}
        {resolveText(shown.yield.unit_text, prefs, def)}
      </p>

      {shown.times && Object.keys(shown.times).length > 0 && (
        <p>
          {Object.entries(shown.times).map(([phase, duration]) => (
            <span key={phase} style={{ marginRight: "1rem" }}>
              <strong>{phase}:</strong> {qty(duration, prefs, def)}
            </span>
          ))}
        </p>
      )}

      <h2>Ingredients</h2>
      <ul className="clean">
        {shown.ingredients.map((ingredient) => (
          <li
            key={ingredient.ref}
            className={
              ingredient.optional ? "ingredient optional" : "ingredient"
            }
          >
            <strong>
              {formatQuantity(ingredient.quantity, prefs, def) ||
                resolveText(ingredient.display, prefs, def)}
            </strong>{" "}
            {resolveText(ingredient.name, prefs, def)}
            {ingredient.preparation &&
              `, ${resolveText(ingredient.preparation, prefs, def)}`}
            {ingredient.optional && <span className="muted"> (optional)</span>}
            {ingredient.substitutes && ingredient.substitutes.length > 0 && (
              <span className="muted">
                {" "}
                — or{" "}
                {ingredient.substitutes
                  .map((s) => resolveText(s.name, prefs, def))
                  .join(", ")}
              </span>
            )}
          </li>
        ))}
      </ul>

      {shown.equipment && shown.equipment.length > 0 && (
        <>
          <h2>Equipment</h2>
          <ul className="clean">
            {shown.equipment.map((item) => (
              <li key={item.ref}>
                {resolveText(item.name, prefs, def)}
                {item.note && (
                  <span className="muted">
                    {" "}
                    — {resolveText(item.note, prefs, def)}
                  </span>
                )}
              </li>
            ))}
          </ul>
        </>
      )}

      <h2>Steps</h2>
      <ol className="clean">
        {shown.steps.map((step, index) => (
          <li key={step.ref} className="step">
            <span className="num">{index + 1}</span>
            <div>
              <Prose prose={resolveProse(step.text, prefs, def)} />
              {step.ingredient_refs && step.ingredient_refs.length > 0 && (
                <p className="muted">
                  Uses: {step.ingredient_refs.map(ingredientName).join(", ")}
                </p>
              )}
              {step.equipment_refs && step.equipment_refs.length > 0 && (
                <p className="muted">
                  With: {step.equipment_refs.map(equipmentName).join(", ")}
                </p>
              )}
              {step.temperature && (
                <p>
                  <strong>Temperature:</strong>{" "}
                  {formatRational(step.temperature.value)}°
                  {step.temperature.unit}
                </p>
              )}
              {step.duration && (
                <StepTimer duration={step.duration} prefs={prefs} def={def} />
              )}
            </div>
          </li>
        ))}
      </ol>

      {shown.notes && (
        <section>
          <h2>Notes</h2>
          <Prose prose={resolveProse(shown.notes, prefs, def)} />
        </section>
      )}

      {shown.attribution && (
        <section className="muted">
          <Prose prose={resolveProse(shown.attribution, prefs, def)} />
        </section>
      )}
    </article>
  );
}
