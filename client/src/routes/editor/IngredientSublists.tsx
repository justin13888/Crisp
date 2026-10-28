/** An ingredient's other measures and substitutes, inside its "More" panel. */
import { useTranslation } from "../../i18n";
import { AmountField, IconButton, TextField } from "./fields";
import { fieldId, type IngredientForm, LIMITS } from "./model";

/** Props shared by the ingredient sublists. */
interface SublistProps {
  /** The ingredient being edited. */
  item: IngredientForm;
  /** Applies a change to the ingredient. */
  patch: (change: Partial<IngredientForm>) => void;
}

/** Alternative measures of the same ingredient (e.g. `120 g`). */
export function OtherMeasures({ item, patch }: SublistProps) {
  const { t } = useTranslation("editorLists");
  const ref = item.ref;
  return (
    <fieldset className="sublist">
      <legend>{t("measure.legend")}</legend>
      {item.alt.map((text, j) => (
        <div className="sublist-row" key={j}>
          <AmountField
            id={fieldId.ingredientAlt(ref, j)}
            label={t("measure.label", { number: j + 1 })}
            hideLabel
            placeholder={t("measure.placeholder")}
            value={text}
            onChange={(value) =>
              patch({
                alt: item.alt.map((a, k) => (k === j ? value : a)),
              })
            }
          />
          <IconButton
            label={t("measure.remove", { number: j + 1 })}
            onClick={() => patch({ alt: item.alt.filter((_, k) => k !== j) })}
          >
            ✕
          </IconButton>
        </div>
      ))}
      {item.alt.length < LIMITS.alt ? (
        <button
          type="button"
          className="secondary"
          onClick={() => {
            patch({ alt: [...item.alt, ""] });
          }}
        >
          {t("measure.add")}
        </button>
      ) : null}
    </fieldset>
  );
}

/** Ingredients that can stand in for this one. */
export function Substitutes({ item, patch }: SublistProps) {
  const { t } = useTranslation("editorLists");
  const ref = item.ref;
  return (
    <fieldset className="sublist">
      <legend>{t("substitute.legend")}</legend>
      {item.substitutes.map((sub, j) => (
        <div className="sublist-row" key={j}>
          <AmountField
            id={fieldId.substitute(ref, j, "amount")}
            label={t("substitute.amount", { number: j + 1 })}
            hideLabel
            placeholder={t("substitute.amountPlaceholder")}
            value={sub.amount}
            onChange={(amount) =>
              patch({
                substitutes: item.substitutes.map((s, k) =>
                  k === j ? { ...s, amount } : s,
                ),
              })
            }
            className="field-amount"
          />
          <TextField
            id={fieldId.substitute(ref, j, "name")}
            label={t("substitute.name", { number: j + 1 })}
            hideLabel
            placeholder={t("substitute.namePlaceholder")}
            value={sub.name}
            onChange={(value) =>
              patch({
                substitutes: item.substitutes.map((s, k) =>
                  k === j ? { ...s, name: value } : s,
                ),
              })
            }
          />
          <IconButton
            label={t("substitute.remove", { number: j + 1 })}
            onClick={() =>
              patch({
                substitutes: item.substitutes.filter((_, k) => k !== j),
              })
            }
          >
            ✕
          </IconButton>
        </div>
      ))}
      {item.substitutes.length < LIMITS.substitutes ? (
        <button
          type="button"
          className="secondary"
          onClick={() =>
            patch({
              substitutes: [...item.substitutes, { name: "", amount: "" }],
            })
          }
        >
          {t("substitute.add")}
        </button>
      ) : null}
    </fieldset>
  );
}
