/**
 * A step card's collapsible panels and the pickers inside them: which
 * ingredients and which equipment the step uses.
 */
import { type ReactNode, useState } from "react";
import { useTranslation } from "../../i18n";
import {
  toggleStepEquipment,
  toggleStepIngredient,
  updateStep,
} from "./actions";
import { useEditor } from "./context";
import { AmountField, CheckField, FieldError, useField } from "./fields";
import { fieldId, type StepForm } from "./model";

/** A collapsible panel inside a step, open at first if it has content. */
export function Panel({
  title,
  count,
  hasContent,
  flagged,
  children,
}: {
  title: string;
  count?: number;
  hasContent: boolean;
  flagged: boolean;
  children: ReactNode;
}) {
  const { t } = useTranslation("editorLists");
  const [initiallyOpen] = useState(hasContent);
  return (
    <details className="step-panel" open={initiallyOpen || undefined}>
      <summary>
        {title}
        {count ? <span className="badge">{count}</span> : null}
        {flagged ? <span className="badge bad">{t("badge.check")}</span> : null}
      </summary>
      <div className="stack-sm step-panel-body">{children}</div>
    </details>
  );
}

/** Which ingredients a step uses, each with an optional partial amount. */
export function IngredientPicker({ step }: { step: StepForm }) {
  const { t } = useTranslation("editorLists");
  const { form, update } = useEditor();
  const id = fieldId.step(step.ref, "ingredients");
  const { messages, a11y } = useField(id);
  const named = form.ingredients.filter(
    (i) => i.name.trim() || i.amount.trim(),
  );
  if (named.length === 0) {
    return <p className="muted hint">{t("picker.ingredients.empty")}</p>;
  }
  return (
    <fieldset
      id={id}
      tabIndex={-1}
      className="picker"
      aria-invalid={a11y["aria-invalid"]}
      aria-describedby={a11y["aria-describedby"]}
    >
      <legend className="visually-hidden">
        {t("picker.ingredients.legend")}
      </legend>
      <ul className="clean stack-sm">
        {named.map((ingredient) => {
          const j = step.ingredients.findIndex((p) => p.ref === ingredient.ref);
          const pick = j >= 0 ? step.ingredients[j] : undefined;
          const label = [ingredient.amount, ingredient.name]
            .filter(Boolean)
            .join(" ");
          return (
            <li key={ingredient.ref} className="picker-row">
              <CheckField
                id={`f-step-${step.ref}-use-${ingredient.ref}`}
                label={label}
                checked={Boolean(pick)}
                onChange={() =>
                  update((f) =>
                    toggleStepIngredient(f, step.ref, ingredient.ref),
                  )
                }
              />
              {pick ? (
                <AmountField
                  id={fieldId.stepPick(step.ref, j)}
                  label={t("picker.ingredients.portion", {
                    name:
                      ingredient.name ||
                      t("picker.ingredients.portionFallback"),
                  })}
                  hideLabel
                  placeholder={t("picker.ingredients.portionPlaceholder")}
                  value={pick.amount}
                  onChange={(amount) =>
                    update((f) =>
                      updateStep(f, step.ref, {
                        ingredients: step.ingredients.map((p, k) =>
                          k === j ? { ...p, amount } : p,
                        ),
                      }),
                    )
                  }
                  className="field-amount"
                />
              ) : null}
            </li>
          );
        })}
      </ul>
      <FieldError id={id} messages={messages} />
    </fieldset>
  );
}

/** Which equipment a step uses. */
export function EquipmentPicker({ step }: { step: StepForm }) {
  const { t } = useTranslation("editorLists");
  const { form, update } = useEditor();
  const id = fieldId.step(step.ref, "equipment");
  const { messages } = useField(id);
  const named = form.equipment.filter((e) => e.name.trim());
  if (named.length === 0) {
    return <p className="muted hint">{t("picker.equipment.empty")}</p>;
  }
  return (
    <div id={id} tabIndex={-1} className="chips">
      {named.map((item) => (
        <button
          key={item.ref}
          type="button"
          className="chip"
          aria-pressed={step.equipment.includes(item.ref)}
          onClick={() =>
            update((f) => toggleStepEquipment(f, step.ref, item.ref))
          }
        >
          {item.name}
        </button>
      ))}
      <FieldError id={id} messages={messages} />
    </div>
  );
}
