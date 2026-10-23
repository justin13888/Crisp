import type { KeyboardEvent } from "react";
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
  isAdvanceKey,
  RowActions,
  hasProblems,
} from "./common";
import { focusField, useEditor } from "./context";
import {
  AmountField,
  CheckField,
  IconButton,
  SelectField,
  TextField,
} from "./fields";
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
  const { form, update, create, focusLater, problems } = useEditor();
  const ref = item.ref;
  const name = item.name.trim() || `ingredient ${number}`;
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
    { value: "", label: "No section" },
    ...form.ingredientGroups.map((g, i) => ({
      value: String(g.key),
      label: g.name.trim() || `Section ${i + 1}`,
    })),
  ];

  return (
    <li className="item-row" data-ref={ref}>
      <div className="item-main ingredient-main">
        <AmountField
          id={fieldId.ingredient(ref, "amount")}
          label={`Amount of ingredient ${number}`}
          hideLabel
          placeholder="1 cup"
          value={item.amount}
          onChange={(amount) => patch({ amount })}
          onKeyDown={advance}
          enterKeyHint="next"
          className="field-amount"
        />
        <TextField
          id={fieldId.ingredient(ref, "name")}
          label={`Ingredient ${number}`}
          hideLabel
          placeholder="flour"
          value={item.name}
          onChange={(value) => patch({ name: value })}
          onKeyDown={advance}
          enterKeyHint="next"
          className="field-name"
        />
        <TextField
          id={fieldId.ingredient(ref, "preparation")}
          label={`Preparation of ingredient ${number}`}
          hideLabel
          placeholder="sifted"
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
          More
          {item.optional ? <span className="badge">optional</span> : null}
          {item.fixed ? <span className="badge">don't scale</span> : null}
          {item.alt.length ? (
            <span className="badge">{item.alt.length} measures</span>
          ) : null}
          {item.substitutes.length ? (
            <span className="badge">{item.substitutes.length} substitutes</span>
          ) : null}
          {moreProblems ? <span className="badge bad">check</span> : null}
        </summary>
        <div className="stack-sm item-more-body">
          <TextField
            id={fieldId.ingredient(ref, "note")}
            label="Note"
            placeholder="or use canned"
            value={item.note}
            onChange={(note) => patch({ note })}
          />
          <div className="cluster">
            <CheckField
              id={`f-ing-${ref}-optional`}
              label="Optional"
              checked={item.optional}
              onChange={(optional) => patch({ optional })}
            />
            <CheckField
              id={`f-ing-${ref}-fixed`}
              label="Don't scale"
              checked={item.fixed}
              onChange={(fixed) => patch({ fixed })}
            />
          </div>
          {form.ingredientGroups.length > 0 ? (
            <SelectField
              id={`f-ing-${ref}-group`}
              label="Section"
              value={item.group === null ? "" : String(item.group)}
              options={groupOptions}
              onChange={(value) =>
                update((f) =>
                  regroup(f, "ingredient", ref, value ? Number(value) : null),
                )
              }
            />
          ) : null}
          <fieldset className="sublist">
            <legend>Other measures</legend>
            {item.alt.map((text, j) => (
              <div className="sublist-row" key={j}>
                <AmountField
                  id={fieldId.ingredientAlt(ref, j)}
                  label={`Other measure ${j + 1}`}
                  hideLabel
                  placeholder="120 g"
                  value={text}
                  onChange={(value) =>
                    patch({
                      alt: item.alt.map((a, k) => (k === j ? value : a)),
                    })
                  }
                />
                <IconButton
                  label={`Remove measure ${j + 1}`}
                  onClick={() =>
                    patch({ alt: item.alt.filter((_, k) => k !== j) })
                  }
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
                Add a measure
              </button>
            ) : null}
          </fieldset>
          <fieldset className="sublist">
            <legend>Substitutes</legend>
            {item.substitutes.map((sub, j) => (
              <div className="sublist-row" key={j}>
                <AmountField
                  id={fieldId.substitute(ref, j, "amount")}
                  label={`Substitute ${j + 1} amount`}
                  hideLabel
                  placeholder="1 cup"
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
                  label={`Substitute ${j + 1}`}
                  hideLabel
                  placeholder="buttermilk"
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
                  label={`Remove substitute ${j + 1}`}
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
                    substitutes: [
                      ...item.substitutes,
                      { name: "", amount: "" },
                    ],
                  })
                }
              >
                Add a substitute
              </button>
            ) : null}
          </fieldset>
          <div className="cluster">
            <button
              type="button"
              className="secondary"
              onClick={() => {
                const created = create((f) => addIngredient(f, { after: ref }));
                focusLater(fieldId.ingredient(created, "amount"));
              }}
            >
              Insert below
            </button>
            <button
              type="button"
              className="secondary"
              onClick={() => {
                const created = create((f) => duplicateIngredient(f, ref));
                focusLater(fieldId.ingredient(created, "name"));
              }}
            >
              Duplicate
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
        + Add ingredient
      </button>
    </>
  );
}

/** Ingredients, grouped into optional sections. */
export function IngredientsSection() {
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
        Ingredients
      </h2>
      <p className="muted hint">
        Type an amount and a name; press Enter for the next row.
      </p>
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
          + Add section
        </button>
      </div>
      <BulkPaste
        title="Paste a list"
        help='One ingredient per line, amount first ("2 cups flour"). A line ending in ":" starts a section.'
        placeholder={
          "2 cups flour\n1 tsp salt\nFor the glaze:\n1 cup icing sugar"
        }
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
