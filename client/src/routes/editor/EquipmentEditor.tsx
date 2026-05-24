/**
 * The editor for a single equipment row. Equipment is fully optional; rows
 * are only added when the user opens the equipment section.
 */

import type { RecipeConstraints } from "../../lib/types";
import { ByteCountedInput } from "./fields";
import type { EquipmentDraft } from "./state";
import type { ErrorMap } from "./validate";

/** Props for [`EquipmentEditor`]. */
export interface EquipmentEditorProps {
  /** Zero-based index into the parent draft's `equipment` array. */
  index: number;
  /** The current row. */
  row: EquipmentDraft;
  /** Called with a patch to merge into `row`. */
  onChange: (patch: Partial<EquipmentDraft>) => void;
  /** Removes this row entirely. */
  onRemove: () => void;
  /** Constraints feed every byte limit. */
  constraints: RecipeConstraints;
  /** Active error map. */
  errors: ErrorMap;
  /** Which field paths the user has touched. */
  touched: Set<string>;
  /** Marks a field path as touched. */
  touch: (path: string) => void;
}

/** Renders one equipment row. */
export function EquipmentEditor({
  index,
  row,
  onChange,
  onRemove,
  constraints,
  errors,
  touched,
  touch,
}: EquipmentEditorProps) {
  const limits = constraints.limits;
  const base = `equipment.${index}`;
  return (
    <div className="row" style={{ marginTop: "0.3rem" }}>
      <ByteCountedInput
        style={{ flex: 2 }}
        value={row.name}
        onChange={(v) => onChange({ name: v })}
        maxBytes={limits.max_text_bytes}
        placeholder="Name (e.g. Mixing bowl)"
        error={errors[`${base}.name`]}
        touched={touched.has(`${base}.name`)}
        onBlur={() => touch(`${base}.name`)}
      />
      <ByteCountedInput
        style={{ flex: 2 }}
        value={row.note}
        onChange={(v) => onChange({ note: v })}
        maxBytes={limits.max_text_bytes}
        placeholder="Note (optional)"
        error={errors[`${base}.note`]}
        touched={touched.has(`${base}.note`)}
        onBlur={() => touch(`${base}.note`)}
      />
      <button type="button" className="secondary" onClick={onRemove}>
        ✕
      </button>
    </div>
  );
}
