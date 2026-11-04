/** Toggle chips for the calendar months a recipe is best made in. */

import { FieldError, useField } from "./fields";

const MONTHS = Array.from({ length: 12 }, (_, i) => i + 1);

/** The month's short name in the cook's locale (`1` → `Jan`). */
export function monthName(month: number, locale?: string): string {
  return new Intl.DateTimeFormat(locale, {
    month: "short",
    timeZone: "UTC",
  }).format(new Date(Date.UTC(2000, month - 1, 1)));
}

/** Toggles `month` in an ascending month list, keeping it ascending. */
export function toggleMonth(selected: readonly number[], month: number) {
  return selected.includes(month)
    ? selected.filter((m) => m !== month)
    : [...selected, month].sort((a, b) => a - b);
}

/** Twelve month chips, January to December. */
export function MonthToggles({
  id,
  label,
  selected,
  onChange,
}: {
  id: string;
  label: string;
  selected: readonly number[];
  onChange: (next: number[]) => void;
}) {
  const { messages, a11y } = useField(id);
  return (
    <fieldset
      id={id}
      tabIndex={-1}
      className="chip-field"
      aria-invalid={a11y["aria-invalid"]}
      aria-describedby={a11y["aria-describedby"]}
    >
      <legend>{label}</legend>
      <div className="chips">
        {MONTHS.map((month) => (
          <button
            key={month}
            type="button"
            className="chip"
            aria-pressed={selected.includes(month)}
            onClick={() => onChange(toggleMonth(selected, month))}
          >
            {monthName(month)}
          </button>
        ))}
      </div>
      <FieldError id={id} messages={messages} />
    </fieldset>
  );
}
