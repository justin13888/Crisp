/**
 * The editor for a single recipe step. The instruction text is primary; the
 * structured fields (group name, duration, temperature, references to
 * ingredients/equipment, intermediate result) are tucked behind a disclosure
 * so they appear only when needed.
 */

import type { RecipeConstraints } from "../../lib/types";
import { ByteCountedInput, ByteCountedTextarea } from "./fields";
import type { EquipmentDraft, IngredientDraft, StepDraft } from "./state";
import type { ErrorMap } from "./validate";

/** Props for [`StepEditor`]. */
export interface StepEditorProps {
  /** Zero-based index into the parent draft's `steps` array. */
  index: number;
  /** The current row. */
  row: StepDraft;
  /** Called with a patch to merge into `row`. */
  onChange: (patch: Partial<StepDraft>) => void;
  /** Removes this row entirely. */
  onRemove: () => void;
  /** Whether the remove button is enabled. */
  canRemove: boolean;
  /** Ingredient rows available for cross-reference. */
  ingredients: IngredientDraft[];
  /** Equipment rows available for cross-reference. */
  equipment: EquipmentDraft[];
  /** Constraints feed every byte limit and array cap. */
  constraints: RecipeConstraints;
  /** Active error map slice, keyed by `steps.<index>.<path>`. */
  errors: ErrorMap;
  /** Which field paths the user has touched. */
  touched: Set<string>;
  /** Marks a field path as touched (called from `onBlur` on every input). */
  touch: (path: string) => void;
}

/** Renders one step row. */
export function StepEditor({
  index,
  row,
  onChange,
  onRemove,
  canRemove,
  ingredients,
  equipment,
  constraints,
  errors,
  touched,
  touch,
}: StepEditorProps) {
  const limits = constraints.limits;
  const base = `steps.${index}`;

  const toggleId = (
    list: number[],
    id: number,
    checked: boolean,
    order: number[],
  ): number[] => {
    const next = new Set(list);
    if (checked) next.add(id);
    else next.delete(id);
    // Preserve the row order so refs stay deterministic.
    return order.filter((rid) => next.has(rid));
  };

  return (
    <div
      className="card"
      style={{ marginTop: "0.5rem", marginBottom: "0.5rem" }}
    >
      <div className="row" style={{ alignItems: "flex-start" }}>
        <div className="step" style={{ marginTop: "0.25rem" }}>
          <div className="num">{index + 1}</div>
        </div>
        <ByteCountedTextarea
          style={{ flex: 1 }}
          value={row.text}
          onChange={(v) => onChange({ text: v })}
          maxBytes={limits.max_text_bytes}
          placeholder={`Step ${index + 1}`}
          error={errors[`${base}.text`]}
          touched={touched.has(`${base}.text`)}
          onBlur={() => touch(`${base}.text`)}
        />
        <button
          type="button"
          className="secondary"
          onClick={onRemove}
          disabled={!canRemove}
          title={canRemove ? "Remove this step" : "Keep at least one step"}
        >
          ✕
        </button>
      </div>

      <details style={{ marginTop: "0.4rem" }}>
        <summary className="muted" style={{ cursor: "pointer" }}>
          More options
        </summary>

        <label>
          <span>Group (e.g. "Dough")</span>
          <ByteCountedInput
            value={row.group}
            onChange={(v) => onChange({ group: v })}
            maxBytes={limits.max_text_bytes}
            error={errors[`${base}.group`]}
            touched={touched.has(`${base}.group`)}
            onBlur={() => touch(`${base}.group`)}
          />
        </label>

        <div className="row">
          <label style={{ flex: 1 }}>
            <span>Duration amount</span>
            <ByteCountedInput
              value={row.durationAmount}
              onChange={(v) => onChange({ durationAmount: v })}
              maxBytes={limits.max_text_bytes}
              placeholder="e.g. 15"
              error={errors[`${base}.durationAmount`]}
              touched={touched.has(`${base}.durationAmount`)}
              onBlur={() => touch(`${base}.durationAmount`)}
            />
          </label>
          <label style={{ flex: 1 }}>
            <span>Duration unit</span>
            <ByteCountedInput
              value={row.durationUnit}
              onChange={(v) => onChange({ durationUnit: v })}
              maxBytes={limits.unit_code_max_bytes}
              placeholder="e.g. min"
              error={errors[`${base}.durationUnit`]}
              touched={touched.has(`${base}.durationUnit`)}
              onBlur={() => touch(`${base}.durationUnit`)}
            />
          </label>
        </div>

        <div className="row">
          <label style={{ flex: 1 }}>
            <span>Temperature value</span>
            <ByteCountedInput
              value={row.temperatureValue}
              onChange={(v) => onChange({ temperatureValue: v })}
              maxBytes={limits.max_text_bytes}
              placeholder="e.g. 180"
              error={errors[`${base}.temperatureValue`]}
              touched={touched.has(`${base}.temperatureValue`)}
              onBlur={() => touch(`${base}.temperatureValue`)}
            />
          </label>
          <label style={{ flex: 1 }}>
            <span>Temperature unit</span>
            <ByteCountedInput
              value={row.temperatureUnit}
              onChange={(v) => onChange({ temperatureUnit: v })}
              maxBytes={limits.unit_code_max_bytes}
              placeholder="e.g. Cel, [degF]"
              error={errors[`${base}.temperatureUnit`]}
              touched={touched.has(`${base}.temperatureUnit`)}
              onBlur={() => touch(`${base}.temperatureUnit`)}
            />
          </label>
        </div>

        <label>
          <span>Produces (intermediate result, e.g. "the dough")</span>
          <ByteCountedInput
            value={row.produces}
            onChange={(v) => onChange({ produces: v })}
            maxBytes={limits.max_text_bytes}
            error={errors[`${base}.produces`]}
            touched={touched.has(`${base}.produces`)}
            onBlur={() => touch(`${base}.produces`)}
          />
        </label>

        {ingredients.length > 0 && (
          <RefPicker
            label="Ingredients used"
            rows={ingredients.map((row) => ({ id: row.id, label: row.name }))}
            selected={row.ingredientIds}
            onChange={(next) => onChange({ ingredientIds: next })}
            toggle={(list, id, checked) =>
              toggleId(
                list,
                id,
                checked,
                ingredients.map((r) => r.id),
              )
            }
            error={errors[`${base}.ingredientIds`]}
          />
        )}

        {equipment.length > 0 && (
          <RefPicker
            label="Equipment used"
            rows={equipment.map((row) => ({ id: row.id, label: row.name }))}
            selected={row.equipmentIds}
            onChange={(next) => onChange({ equipmentIds: next })}
            toggle={(list, id, checked) =>
              toggleId(
                list,
                id,
                checked,
                equipment.map((r) => r.id),
              )
            }
            error={errors[`${base}.equipmentIds`]}
          />
        )}
      </details>
    </div>
  );
}

/** Compact checkbox picker for `ingredient_refs` / `equipment_refs`. */
function RefPicker({
  label,
  rows,
  selected,
  onChange,
  toggle,
  error,
}: {
  label: string;
  rows: { id: number; label: string }[];
  selected: number[];
  onChange: (next: number[]) => void;
  toggle: (list: number[], id: number, checked: boolean) => number[];
  error?: string;
}) {
  const selectedSet = new Set(selected);
  return (
    <div style={{ marginTop: "0.4rem" }}>
      <span className="muted" style={{ fontSize: "0.85rem" }}>
        {label}
      </span>
      <div style={{ display: "flex", flexWrap: "wrap", gap: "0.4rem 1rem" }}>
        {rows.map((r) => {
          const checked = selectedSet.has(r.id);
          return (
            <label
              key={r.id}
              style={{
                display: "flex",
                alignItems: "center",
                margin: 0,
                gap: "0.35rem",
              }}
            >
              <input
                type="checkbox"
                checked={checked}
                onChange={(e) =>
                  onChange(toggle(selected, r.id, e.target.checked))
                }
                style={{ width: "auto", margin: 0 }}
              />
              <span style={{ margin: 0 }}>{r.label || "(unnamed)"}</span>
            </label>
          );
        })}
      </div>
      {error && (
        <div style={{ color: "var(--bad)", fontSize: "0.78rem" }}>{error}</div>
      )}
    </div>
  );
}
