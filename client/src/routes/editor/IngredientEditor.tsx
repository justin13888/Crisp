/**
 * The editor for a single ingredient row. Renders name/amount/unit on the
 * front line and tucks the optional fields (display override, preparation,
 * optional flag, group, substitutes) behind a `<details>` disclosure so the
 * primary layout stays compact.
 */

import type { RecipeConstraints } from "../../lib/types";
import { ByteCountedInput } from "./fields";
import {
  emptySubstitute,
  type IngredientDraft,
  type SubstituteDraft,
} from "./state";
import type { ErrorMap } from "./validate";

/** Props for [`IngredientEditor`]. */
export interface IngredientEditorProps {
  /** Zero-based index into the parent draft's `ingredients` array. */
  index: number;
  /** The current row. */
  row: IngredientDraft;
  /** Called with a patch to merge into `row`. */
  onChange: (patch: Partial<IngredientDraft>) => void;
  /** Removes this row entirely. */
  onRemove: () => void;
  /** Whether the remove button is enabled (false to keep at least one row). */
  canRemove: boolean;
  /** Names of every ingredient group declared at the recipe level. */
  ingredientGroupNames: string[];
  /** Constraints feed every byte limit and array cap. */
  constraints: RecipeConstraints;
  /** Active error map slice, keyed by `ingredients.<index>.<path>`. */
  errors: ErrorMap;
  /** Which field paths the user has touched. */
  touched: Set<string>;
  /** Marks a field path as touched (called from `onBlur` on every input). */
  touch: (path: string) => void;
}

/** Renders one ingredient row. */
export function IngredientEditor({
  index,
  row,
  onChange,
  onRemove,
  canRemove,
  ingredientGroupNames,
  constraints,
  errors,
  touched,
  touch,
}: IngredientEditorProps) {
  const limits = constraints.limits;
  const base = `ingredients.${index}`;
  const subsAtLimit = row.substitutes.length >= limits.max_substitutes;

  return (
    <div
      className="card"
      style={{ marginTop: "0.5rem", marginBottom: "0.5rem" }}
    >
      <div className="row">
        <ByteCountedInput
          style={{ flex: 2, minWidth: "12rem" }}
          value={row.name}
          onChange={(v) => onChange({ name: v })}
          maxBytes={limits.max_text_bytes}
          placeholder="Name (e.g. Flour)"
          error={errors[`${base}.name`]}
          touched={touched.has(`${base}.name`)}
          onBlur={() => touch(`${base}.name`)}
        />
        <ByteCountedInput
          style={{ flex: 1, minWidth: "6rem" }}
          value={row.amount}
          onChange={(v) => onChange({ amount: v })}
          maxBytes={limits.max_text_bytes}
          placeholder="Amount"
        />
        <ByteCountedInput
          style={{ flex: 1, minWidth: "6rem" }}
          value={row.unit}
          onChange={(v) => onChange({ unit: v })}
          maxBytes={limits.unit_code_max_bytes}
          placeholder="Unit (e.g. g, cup)"
          error={errors[`${base}.unit`]}
          touched={touched.has(`${base}.unit`)}
          onBlur={() => touch(`${base}.unit`)}
        />
        <button
          type="button"
          className="secondary"
          onClick={onRemove}
          disabled={!canRemove}
          title={
            canRemove
              ? "Remove this ingredient"
              : "Keep at least one ingredient"
          }
        >
          ✕
        </button>
      </div>

      <details style={{ marginTop: "0.4rem" }}>
        <summary className="muted" style={{ cursor: "pointer" }}>
          More options
        </summary>

        <label>
          <span>Display amount override (e.g. "about 2 cups")</span>
          <ByteCountedInput
            value={row.display}
            onChange={(v) => onChange({ display: v })}
            maxBytes={limits.max_text_bytes}
            error={errors[`${base}.display`]}
            touched={touched.has(`${base}.display`)}
            onBlur={() => touch(`${base}.display`)}
          />
        </label>

        <label>
          <span>Preparation note (e.g. "finely chopped")</span>
          <ByteCountedInput
            value={row.preparation}
            onChange={(v) => onChange({ preparation: v })}
            maxBytes={limits.max_text_bytes}
            error={errors[`${base}.preparation`]}
            touched={touched.has(`${base}.preparation`)}
            onBlur={() => touch(`${base}.preparation`)}
          />
        </label>

        <label className="row" style={{ alignItems: "center" }}>
          <input
            type="checkbox"
            style={{ width: "auto" }}
            checked={row.optional}
            onChange={(e) => onChange({ optional: e.target.checked })}
          />
          <span style={{ margin: 0 }}>Optional ingredient</span>
        </label>

        {ingredientGroupNames.length > 0 && (
          <label>
            <span>Group</span>
            <select
              value={row.group ?? ""}
              onChange={(e) =>
                onChange({
                  group: e.target.value === "" ? null : Number(e.target.value),
                })
              }
            >
              <option value="">— ungrouped —</option>
              {ingredientGroupNames.map((name, i) => (
                <option key={i} value={i}>
                  {name || `Group ${i + 1}`}
                </option>
              ))}
            </select>
          </label>
        )}

        <div style={{ marginTop: "0.5rem" }}>
          <span className="muted" style={{ fontSize: "0.85rem" }}>
            Substitutes
          </span>
          {row.substitutes.map((sub, j) => (
            <SubstituteRow
              key={sub.id}
              ingredientIndex={index}
              substituteIndex={j}
              row={sub}
              onChange={(patch) =>
                onChange({
                  substitutes: row.substitutes.map((s, k) =>
                    k === j ? { ...s, ...patch } : s,
                  ),
                })
              }
              onRemove={() =>
                onChange({
                  substitutes: row.substitutes.filter((_, k) => k !== j),
                })
              }
              constraints={constraints}
              errors={errors}
              touched={touched}
              touch={touch}
            />
          ))}
          <button
            type="button"
            className="secondary"
            onClick={() =>
              onChange({
                substitutes: [...row.substitutes, emptySubstitute()],
              })
            }
            disabled={subsAtLimit}
            title={
              subsAtLimit
                ? `Limit reached (${limits.max_substitutes})`
                : undefined
            }
          >
            Add substitute
          </button>
        </div>
      </details>
    </div>
  );
}

/** A single substitute row. Kept inline to share state plumbing. */
function SubstituteRow({
  ingredientIndex,
  substituteIndex,
  row,
  onChange,
  onRemove,
  constraints,
  errors,
  touched,
  touch,
}: {
  ingredientIndex: number;
  substituteIndex: number;
  row: SubstituteDraft;
  onChange: (patch: Partial<SubstituteDraft>) => void;
  onRemove: () => void;
  constraints: RecipeConstraints;
  errors: ErrorMap;
  touched: Set<string>;
  touch: (path: string) => void;
}) {
  const limits = constraints.limits;
  const base = `ingredients.${ingredientIndex}.substitutes.${substituteIndex}`;
  return (
    <div className="row" style={{ marginTop: "0.3rem" }}>
      <ByteCountedInput
        style={{ flex: 2 }}
        value={row.name}
        onChange={(v) => onChange({ name: v })}
        maxBytes={limits.max_text_bytes}
        placeholder="Substitute name"
        error={errors[`${base}.name`]}
        touched={touched.has(`${base}.name`)}
        onBlur={() => touch(`${base}.name`)}
      />
      <ByteCountedInput
        style={{ flex: 1 }}
        value={row.amount}
        onChange={(v) => onChange({ amount: v })}
        maxBytes={limits.max_text_bytes}
        placeholder="Amount"
      />
      <ByteCountedInput
        style={{ flex: 1 }}
        value={row.unit}
        onChange={(v) => onChange({ unit: v })}
        maxBytes={limits.unit_code_max_bytes}
        placeholder="Unit"
        error={errors[`${base}.unit`]}
        touched={touched.has(`${base}.unit`)}
        onBlur={() => touch(`${base}.unit`)}
      />
      <button type="button" className="secondary" onClick={onRemove}>
        ✕
      </button>
    </div>
  );
}
