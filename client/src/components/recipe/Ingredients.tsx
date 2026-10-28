import { useTranslation } from "../../i18n";
import type {
  EquipmentView,
  IngredientSection,
  IngredientView,
} from "../../lib/crisp";
import { miseKey } from "./model";

/** Checklist wiring for {@link IngredientSections}. */
export interface IngredientChecklist {
  /** Checked keys (see `miseKey`). */
  checked: ReadonlySet<number>;
  /** Toggles one ingredient by `ref`. */
  onToggle(ref: number): void;
}

/** The markers and secondary text of one ingredient. */
function IngredientDetail({ item }: { item: IngredientView }) {
  const { t } = useTranslation("recipe");
  return (
    <>
      {(item.optional || item.fixed) && (
        <span className="ingredient__flags">
          {item.optional && (
            <span className="badge">{t("ingredient.optional")}</span>
          )}
          {item.fixed && (
            <span className="badge" title={t("ingredient.fixedHint")}>
              {t("ingredient.fixed")}
            </span>
          )}
        </span>
      )}
      {item.alt.length > 0 && (
        <span className="ingredient__alt muted"> ({item.alt.join(" · ")})</span>
      )}
      {item.note && <span className="ingredient__note muted">{item.note}</span>}
      {item.substitutes.length > 0 && (
        <span className="ingredient__subs muted">
          {t("ingredient.substitutes", {
            substitutes: item.substitutes.join("; "),
          })}
        </span>
      )}
    </>
  );
}

/** The amount, name, and preparation of one ingredient. */
function IngredientMain({ item }: { item: IngredientView }) {
  return (
    <>
      {item.amount && (
        <span className="ingredient__amount tabular">{item.amount}</span>
      )}{" "}
      <span className="ingredient__name">{item.name}</span>
      {item.preparation && (
        <span className="ingredient__prep">, {item.preparation}</span>
      )}
    </>
  );
}

/**
 * Ingredients grouped by section. With `checklist`, every row is one big
 * tap target that checks the ingredient off.
 */
export function IngredientSections({
  sections,
  checklist,
  headingLevel = 3,
}: {
  sections: readonly IngredientSection[];
  checklist?: IngredientChecklist;
  headingLevel?: 3 | 4;
}) {
  const Heading = headingLevel === 3 ? "h3" : "h4";
  return (
    <div className="ingredient-sections">
      {sections.map((section, i) => (
        <div key={i} className="ingredient-section">
          {section.title && (
            <Heading className="section-title">{section.title}</Heading>
          )}
          {checklist ? (
            <ul className="cook-checklist">
              {section.items.map((item) => {
                const checked = checklist.checked.has(miseKey(item.ref));
                return (
                  <li
                    key={item.ref}
                    className={`ingredient${item.optional ? " optional" : ""}`}
                  >
                    <label data-checked={checked}>
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => checklist.onToggle(item.ref)}
                      />
                      <span className="check-text">
                        <IngredientMain item={item} />
                        <IngredientDetail item={item} />
                      </span>
                    </label>
                  </li>
                );
              })}
            </ul>
          ) : (
            <ul className="ingredient-list clean">
              {section.items.map((item) => (
                <li
                  key={item.ref}
                  className={`ingredient${item.optional ? " optional" : ""}`}
                >
                  <IngredientMain item={item} />
                  <IngredientDetail item={item} />
                </li>
              ))}
            </ul>
          )}
        </div>
      ))}
    </div>
  );
}

/** The equipment list. */
export function EquipmentList({
  equipment,
}: {
  equipment: readonly EquipmentView[];
}) {
  if (equipment.length === 0) return null;
  return (
    <ul className="equipment-list clean">
      {equipment.map((item) => (
        <li key={item.ref}>
          {item.amount && <span className="tabular">{item.amount} × </span>}
          {item.name}
          {item.note && <span className="muted">, {item.note}</span>}
        </li>
      ))}
    </ul>
  );
}
