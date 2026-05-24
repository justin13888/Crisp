/**
 * A checkbox group bound to a closed enum value list from `RecipeConstraints`.
 *
 * The list of codes is owned by `crisp-core` — the component never invents or
 * humanizes codes on its own; it just renders the wire strings it is handed
 * and presents them through [`humanizeCode`] for display.
 */

import { humanizeCode } from "./fields";

/** Props for [`EnumMultiSelect`]. */
export interface EnumMultiSelectProps {
  /** The closed enum's wire-string list (from `constraints.enums.*`). */
  options: readonly string[];
  /** Currently selected wire codes. */
  selected: string[];
  /** Called with the next selection on every change. */
  onChange: (next: string[]) => void;
  /** Optional hard cap; checkboxes for unselected codes disable past it. */
  maxSelected?: number;
  /** Optional id stem for label/checkbox `id` pairs. */
  idPrefix?: string;
}

/** Renders one checkbox per code, capped by `maxSelected` when set. */
export function EnumMultiSelect({
  options,
  selected,
  onChange,
  maxSelected,
  idPrefix,
}: EnumMultiSelectProps) {
  const selectedSet = new Set(selected);
  const capped = maxSelected !== undefined && selectedSet.size >= maxSelected;

  const toggle = (code: string, checked: boolean) => {
    const next = new Set(selectedSet);
    if (checked) next.add(code);
    else next.delete(code);
    // Preserve the canonical (source) order for serialization stability.
    onChange(options.filter((c) => next.has(c)));
  };

  return (
    <div style={{ display: "flex", flexWrap: "wrap", gap: "0.4rem 1rem" }}>
      {options.map((code) => {
        const checked = selectedSet.has(code);
        const disabled = !checked && capped;
        const id = idPrefix ? `${idPrefix}-${code}` : undefined;
        return (
          <label
            key={code}
            htmlFor={id}
            style={{
              display: "flex",
              alignItems: "center",
              margin: 0,
              gap: "0.35rem",
              color: disabled ? "var(--muted)" : "var(--ink)",
              cursor: disabled ? "not-allowed" : "pointer",
            }}
          >
            <input
              id={id}
              type="checkbox"
              checked={checked}
              disabled={disabled}
              onChange={(e) => toggle(code, e.target.checked)}
              style={{ width: "auto", margin: 0 }}
            />
            <span style={{ margin: 0 }}>{humanizeCode(code)}</span>
          </label>
        );
      })}
    </div>
  );
}
