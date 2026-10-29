import { type FormEvent, useId, useState } from "react";
import { useEngine } from "../../hooks/useEngine";
import { useTranslation } from "../../i18n";
import type { Scale, UnitSystem } from "../../lib/cookSession";
import type { Rational } from "../../lib/crisp";
import {
  formatScale,
  SCALE_PRESETS,
  sameScale,
  scaleForServings,
  stepScale,
  UNIT_OPTIONS,
} from "./model";

/** Props for {@link ScaleControls}. */
export interface ScaleControlsProps {
  /** The current multiplier. */
  scale: Scale;
  /** The current unit system. */
  unitSystem: UnitSystem;
  /** The engine's scaled yield text, e.g. `"48 cookies"`. */
  yieldText: string;
  /** The scaled numeric yield, when the recipe has one. */
  yieldAmount: Rational | null | undefined;
  /** The yield's unit, e.g. `"cookies"`. */
  yieldUnit: string;
  /** The language the view was resolved in (for parsing "make N"). */
  language: string;
  /** The scale that yields `target`, from the bundle (`factorForYield`). */
  factorForYield(target: Rational): Rational | undefined;
  /** Sets the multiplier. */
  onScale(scale: Scale): void;
  /** Sets the unit system. */
  onUnits(units: UnitSystem): void;
}

/**
 * The serving scaler (stepper, presets, "make N") and the units toggle.
 * Values are only ever the multiplier; amounts re-render from the engine.
 */
export function ScaleControls({
  scale,
  unitSystem,
  yieldText,
  yieldAmount,
  yieldUnit,
  language,
  factorForYield,
  onScale,
  onUnits,
}: ScaleControlsProps) {
  const id = useId();
  const { t } = useTranslation("recipe");
  const { engine } = useEngine();
  const [servings, setServings] = useState("");
  const [invalid, setInvalid] = useState(false);
  const down = stepScale(scale, -1);
  const up = stepScale(scale, 1);
  const hasYield = yieldAmount !== null && yieldAmount !== undefined;

  const submit = (event: FormEvent): void => {
    event.preventDefault();
    const next =
      engine && scaleForServings(servings, language, engine, factorForYield);
    setInvalid(next === null);
    if (next) {
      onScale(next);
      setServings("");
    }
  };

  return (
    <div className="scale-controls">
      <div className="scale-controls__row">
        <fieldset className="stepper">
          <legend className="visually-hidden">{t("scale.multiplier")}</legend>
          <button
            type="button"
            className="secondary icon"
            disabled={!down}
            onClick={() => down && onScale(down)}
            aria-label={t("scale.fewer")}
          >
            −
          </button>
          <output className="stepper__value" aria-live="polite">
            {engine && <strong>{formatScale(scale, engine)}</strong>}
            {yieldText && <span className="muted">{yieldText}</span>}
          </output>
          <button
            type="button"
            className="secondary icon"
            disabled={!up}
            onClick={() => up && onScale(up)}
            aria-label={t("scale.more")}
          >
            +
          </button>
        </fieldset>
        <fieldset className="chips">
          <legend className="visually-hidden">{t("scale.presets")}</legend>
          {SCALE_PRESETS.map((preset) => (
            <button
              key={preset.label}
              type="button"
              className="chip"
              aria-pressed={sameScale(preset.scale, scale)}
              onClick={() => onScale(preset.scale)}
            >
              {preset.label}
            </button>
          ))}
        </fieldset>
      </div>
      <div className="scale-controls__row">
        {hasYield && (
          <form className="make-n" onSubmit={submit}>
            <label htmlFor={`${id}-n`}>{t("scale.make")}</label>
            <input
              id={`${id}-n`}
              type="text"
              inputMode="decimal"
              autoComplete="off"
              value={servings}
              placeholder={
                yieldAmount && yieldAmount[1] === 1
                  ? String(yieldAmount[0])
                  : ""
              }
              aria-invalid={invalid}
              onChange={(e) => {
                setServings(e.target.value);
                setInvalid(false);
              }}
            />
            <span className="make-n__unit">{yieldUnit}</span>
            <button type="submit" className="secondary">
              {t("scale.set")}
            </button>
          </form>
        )}
        <fieldset className="segmented">
          <legend className="visually-hidden">{t("scale.units")}</legend>
          {UNIT_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              aria-pressed={unitSystem === option.value}
              onClick={() => onUnits(option.value)}
            >
              {option.label}
            </button>
          ))}
        </fieldset>
      </div>
      {invalid && (
        <p className="field-error" role="alert">
          {t("scale.invalid")}
        </p>
      )}
    </div>
  );
}
