import type { KeyboardEvent } from "react";
import { useTranslation } from "../../i18n";
import {
  addBulkIngredients,
  addGroup,
  addIngredient,
  duplicateIngredient,
  moveIngredient,
  regroup,
  removeIngredient,
  updateIngredient,
} from "./actions";
import { amountPredicate, parseIngredientList } from "./bulk";
import {
  BulkPaste,
  GroupHeader,
  hasProblems,
  isAdvanceKey,
  RowActions,
} from "./common";
import { focusField, useEditor } from "./context";
import { AmountField, CheckField, SelectField, TextField } from "./fields";
import { OtherMeasures, Substitutes } from "./IngredientSublists";
import { fieldId, type IngredientForm, LIMITS } from "./model";

/** One ingredient: amount, name, preparation, and a "More" panel. */
function IngredientRow({
  item,
  number,
  first,
  last,
  next,
}: {
  item: IngredientForm;
  number: number;
  first: boolean;
  last: boolean;
  next: IngredientForm | undefined;
}) {
  const { t } = useTranslation("editorLists");
  const { form, update, create, focusLater, problems } = useEditor();
  const ref = item.ref;
  const name = item.name.trim() || t("ingredient.rowName", { number });
  const patch = (change: Partial<IngredientForm>) =>
    update((f) => updateIngredient(f, ref, change));
  const moreProblems = hasProblems(
    problems,
    [`f-ing-${ref}-`],
    [
      fieldId.ingredient(ref, "name"),
      fieldId.ingredient(ref, "amount"),
      fieldId.ingredient(ref, "preparation"),
    ],
  );

  const advance = (event: KeyboardEvent<HTMLInputElement>) => {
    if (!isAdvanceKey(event)) return;
    event.preventDefault();
    if (next) {
      focusField(fieldId.ingredient(next.ref, "amount"));
      return;
    }
    const created = create((f) => addIngredient(f, { after: ref }));
    focusLater(fieldId.ingredient(created, "amount"));
  };

  const groupOptions = [
    { value: "", label: t("section.none") },
    ...form.ingredientGroups.map((g, i) => ({
      value: String(g.key),
      label: g.name.trim() || t("section.numbered", { number: i + 1 }),
    })),
  ];

  return (
    <li className="item-row" data-ref={ref}>
      <div className="item-main ingredient-main">
        <AmountField
          id={fieldId.ingredient(ref, "amount")}
          label={t("ingredient.amount", { number })}
          hideLabel
          placeholder={t("ingredient.amountPlaceholder")}
          value={item.amount}
          onChange={(amount) => patch({ amount })}
          onKeyDown={advance}
          enterKeyHint="next"
          className="field-amount"
        />
        <TextField
          id={fieldId.ingredient(ref, "name")}
          label={t("ingredient.name", { number })}
          hideLabel
          placeholder={t("ingredient.namePlaceholder")}
          value={item.name}
          onChange={(value) => patch({ name: value })}
          onKeyDown={advance}
          enterKeyHint="next"
          className="field-name"
        />
        <TextField
          id={fieldId.ingredient(ref, "preparation")}
          label={t("ingredient.preparation", { number })}
          hideLabel
          placeholder={t("ingredient.preparationPlaceholder")}
          value={item.preparation}
          onChange={(preparation) => patch({ preparation })}
          onKeyDown={advance}
          enterKeyHint="next"
          className="field-prep"
        />
        <RowActions
          name={name}
          first={first}
          last={last}
          onMove={(dir) => update((f) => moveIngredient(f, ref, dir))}
          onDelete={() => update((f) => removeIngredient(f, ref))}
        />
      </div>
      <details className="item-more">
        <summary>
          {t("row.more")}
          {item.optional ? (
            <span className="badge">{t("ingredient.badge.optional")}</span>
          ) : null}
          {item.fixed ? (
            <span className="badge">{t("ingredient.badge.fixed")}</span>
          ) : null}
          {item.alt.length ? (
            <span className="badge">
              {t("ingredient.badge.measures", { count: item.alt.length })}
            </span>
          ) : null}
          {item.substitutes.length ? (
            <span className="badge">
              {t("ingredient.badge.substitutes", {
                count: item.substitutes.length,
              })}
            </span>
          ) : null}
          {moreProblems ? (
            <span className="badge bad">{t("badge.check")}</span>
          ) : null}
        </summary>
        <div className="stack-sm item-more-body">
          <TextField
            id={fieldId.ingredient(ref, "note")}
            label={t("ingredient.note")}
            placeholder={t("ingredient.notePlaceholder")}
            value={item.note}
            onChange={(note) => patch({ note })}
          />
          <div className="cluster">
            <CheckField
              id={`f-ing-${ref}-optional`}
              label={t("ingredient.optional")}
              checked={item.optional}
              onChange={(optional) => patch({ optional })}
            />
            <CheckField
              id={`f-ing-${ref}-fixed`}
              label={t("ingredient.fixed")}
              checked={item.fixed}
              onChange={(fixed) => patch({ fixed })}
            />
          </div>
          {form.ingredientGroups.length > 0 ? (
            <SelectField
              id={`f-ing-${ref}-group`}
              label={t("section.label")}
              value={item.group === null ? "" : String(item.group)}
              options={groupOptions}
              onChange={(value) =>
                update((f) =>
                  regroup(f, "ingredient", ref, value ? Number(value) : null),
                )
              }
            />
          ) : null}
          <OtherMeasures item={item} patch={patch} />
          <Substitutes item={item} patch={patch} />
          <div className="cluster">
            <button
              type="button"
              className="secondary"
              onClick={() => {
                const created = create((f) => addIngredient(f, { after: ref }));
                focusLater(fieldId.ingredient(created, "amount"));
              }}
            >
              {t("ingredient.insertBelow")}
            </button>
            <button
              type="button"
              className="secondary"
              onClick={() => {
                const created = create((f) => duplicateIngredient(f, ref));
                focusLater(fieldId.ingredient(created, "name"));
              }}
            >
              {t("row.duplicate")}
            </button>
          </div>
        </div>
      </details>
    </li>
  );
}

/** The ingredients of one section (or the unsectioned ones). */
function Lane({
  group,
  items,
  startNumber,
}: {
  group: number | null;
  items: IngredientForm[];
  startNumber: number;
}) {
  const { t } = useTranslation("editorLists");
  const { create, focusLater } = useEditor();
  return (
    <>
      <ol className="item-list clean">
        {items.map((item, i) => (
          <IngredientRow
            key={item.ref}
            item={item}
            number={startNumber + i}
            first={i === 0}
            last={i === items.length - 1}
            next={items[i + 1]}
          />
        ))}
      </ol>
      <button
        type="button"
        className="secondary add-row"
        onClick={() => {
          const created = create((f) => addIngredient(f, { group }));
          focusLater(fieldId.ingredient(created, "amount"));
        }}
      >
        {t("ingredients.add")}
      </button>
    </>
  );
}

/** Ingredients, grouped into optional sections. */
export function IngredientsSection() {
  const { t } = useTranslation("editorLists");
  const { form, update, create, engine, focusLater } = useEditor();
  const ungrouped = form.ingredients.filter((i) => i.group === null);
  let number = ungrouped.length + 1;
  return (
    <section
      id="sec-ingredients"
      className="editor-section"
      aria-labelledby="h-ingredients"
    >
      <h2 id="h-ingredients" tabIndex={-1}>
        {t("ingredients.heading")}
      </h2>
      <p className="muted hint">{t("ingredients.hint")}</p>
      <div id={fieldId.ingredients} tabIndex={-1} className="anchor" />
      {ungrouped.length > 0 || form.ingredientGroups.length === 0 ? (
        <Lane group={null} items={ungrouped} startNumber={1} />
      ) : null}
      {form.ingredientGroups.map((group, gi) => {
        const items = form.ingredients.filter((i) => i.group === group.key);
        const start = number;
        number += items.length;
        return (
          <div className="group" key={group.key}>
            <GroupHeader
              kind="ingredient"
              group={group}
              fieldId={fieldId.ingredientGroup(group.key)}
              first={gi === 0}
              last={gi === form.ingredientGroups.length - 1}
            />
            <Lane group={group.key} items={items} startNumber={start} />
          </div>
        );
      })}
      <div className="cluster section-tools">
        <button
          type="button"
          className="secondary"
          disabled={form.ingredientGroups.length >= LIMITS.groups}
          onClick={() => {
            const created = create((f) => {
              const g = addGroup(f, "ingredient");
              return {
                form: addIngredient(g.form, { group: g.ref }).form,
                ref: g.ref,
              };
            });
            focusLater(fieldId.ingredientGroup(created));
          }}
        >
          {t("section.add")}
        </button>
      </div>
      <BulkPaste
        title={t("ingredients.paste.title")}
        help={t("ingredients.paste.help")}
        placeholder={t("ingredients.paste.placeholder")}
        onApply={(text) => {
          const sections = parseIngredientList(
            text,
            amountPredicate(engine.parseAmount, form.lang),
          );
          update((f) => addBulkIngredients(f, sections));
        }}
      />
    </section>
  );
}
