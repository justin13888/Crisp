import { currentLocale, useTranslation } from "../../i18n";
import {
  type Engine,
  type EquipmentView,
  type IngredientSection,
  type IngredientView,
  loadedEngine,
} from "../../lib/crisp";
import { useDerivedMeasures } from "../settings/derivedMeasures";
import { miseKey } from "./model";

/** Checklist wiring for {@link IngredientSections}. */
export interface IngredientChecklist {
  /** Checked keys (see `miseKey`). */
  checked: ReadonlySet<number>;
  /** Toggles one ingredient by `ref`. */
  onToggle(ref: number): void;
}

/**
 * The markers and secondary text of one ingredient. A density-derived
 * measure appears only when the reader turned it on and the author gave no
 * `alt` of their own: an author's measure always wins.
 */
function IngredientDetail({ item }: { item: IngredientView }) {
  const { t } = useTranslation("recipe");
  const showDerived = useDerivedMeasures();
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
      {showDerived && item.alt.length === 0 && item.derived && (
        <span
          className="ingredient__alt ingredient__derived muted"
          title={t("ingredient.derivedHint")}
        >
          {" "}
          ({item.derived})
        </span>
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

/** The primary language subtag of a BCP 47 tag (`zh-Hant-TW` → `zh`). */
function primaryLanguage(tag: string): string {
  return (tag.split("-")[0] ?? "").toLowerCase();
}

/**
 * The ingredient index's name for a coded ingredient in the reader's
 * locale, to show beside the recipe's own name, or `undefined` when there
 * is nothing worth adding: the recipe is already in the reader's language,
 * the index has no name in it (only a fallback in another language), the
 * code is unknown, or the name is the one the recipe gives.
 */
export function readerIndexName(
  engine: Pick<Engine, "ingredientName"> | null,
  item: Pick<IngredientView, "name" | "code">,
  recipeLanguage: string | undefined,
  readerLocale: string,
): string | undefined {
  if (!engine || !item.code || !recipeLanguage) return undefined;
  const reader = primaryLanguage(readerLocale);
  if (primaryLanguage(recipeLanguage) === reader) return undefined;
  const found = engine.ingredientName(item.code, readerLocale);
  if (!found || primaryLanguage(found.shard) !== reader) return undefined;
  const same = found.name.toLocaleLowerCase() === item.name.toLocaleLowerCase();
  return same ? undefined : found.name;
}

/** The amount, name, and preparation of one ingredient. */
function IngredientMain({
  item,
  language,
}: {
  item: IngredientView;
  language: string | undefined;
}) {
  // Subscribing to the interface language re-renders on a locale change.
  useTranslation("recipe");
  const reader = currentLocale();
  const indexName = readerIndexName(loadedEngine(), item, language, reader);
  return (
    <>
      {item.amount && (
        <span className="ingredient__amount tabular">{item.amount}</span>
      )}{" "}
      <span className="ingredient__name">{item.name}</span>
      {indexName && (
        <span className="ingredient__index-name muted" lang={reader}>
          {" "}
          ({indexName})
        </span>
      )}
      {item.preparation && (
        <span className="ingredient__prep">, {item.preparation}</span>
      )}
    </>
  );
}

/**
 * Ingredients grouped by section. With `checklist`, every row is one big
 * tap target that checks the ingredient off. With `language` (the language
 * the view was resolved in), a coded ingredient also shows the index's name
 * in the reader's locale when the recipe is in another language.
 */
export function IngredientSections({
  sections,
  checklist,
  headingLevel = 3,
  language,
}: {
  sections: readonly IngredientSection[];
  checklist?: IngredientChecklist;
  headingLevel?: 3 | 4;
  language?: string;
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
                        <IngredientMain item={item} language={language} />
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
                  <IngredientMain item={item} language={language} />
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
