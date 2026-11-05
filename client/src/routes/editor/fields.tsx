/**
 * Form controls wired to validation: each renders its problems under the
 * control with `aria-invalid` and `aria-describedby`.
 */

import {
  type InputHTMLAttributes,
  type ReactNode,
  useMemo,
  useState,
} from "react";
import type { Quantity } from "../../generated/crisp";
import { useTranslation } from "../../i18n";
import { useEditor } from "./context";
import { FieldError, useField } from "./fieldState";
import { addSlugs } from "./slugs";

export { type FieldA11y, FieldError, useField } from "./fieldState";

/** Props shared by text controls. */
interface TextProps {
  /** DOM id (also the validation field id). */
  id: string;
  /** Visible label (or accessible name when `hideLabel`). */
  label: string;
  /** Current value. */
  value: string;
  /** Change handler. */
  onChange: (value: string) => void;
  /** Hide the label visually (still announced). */
  hideLabel?: boolean;
  /** Placeholder. */
  placeholder?: string;
  /** Extra content under the control (previews, hints). */
  hint?: ReactNode;
  /** Wrapper class. */
  className?: string;
}

/** Props of {@link TextField}. */
export type TextFieldProps = TextProps &
  Pick<
    InputHTMLAttributes<HTMLInputElement>,
    "onKeyDown" | "inputMode" | "type" | "autoComplete" | "enterKeyHint"
  >;

/** A labeled single-line input. */
export function TextField({
  id,
  label,
  value,
  onChange,
  hideLabel,
  placeholder,
  hint,
  className,
  onKeyDown,
  inputMode,
  type,
  autoComplete,
  enterKeyHint,
}: TextFieldProps) {
  const { messages, a11y } = useField(id);
  return (
    <div className={`field ${className ?? ""}`}>
      <label htmlFor={id}>
        <span className={hideLabel ? "visually-hidden" : undefined}>
          {label}
        </span>
      </label>
      <input
        id={id}
        type={type ?? "text"}
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.currentTarget.value)}
        onKeyDown={onKeyDown}
        inputMode={inputMode}
        autoComplete={autoComplete ?? "off"}
        enterKeyHint={enterKeyHint}
        {...a11y}
      />
      {hint}
      <FieldError id={id} messages={messages} />
    </div>
  );
}

/** A labeled multi-line input; blank lines separate paragraphs. */
export function TextArea({
  id,
  label,
  value,
  onChange,
  hideLabel,
  placeholder,
  hint,
  className,
  rows,
}: TextProps & { rows?: number }) {
  const { messages, a11y } = useField(id);
  return (
    <div className={`field ${className ?? ""}`}>
      <label htmlFor={id}>
        <span className={hideLabel ? "visually-hidden" : undefined}>
          {label}
        </span>
      </label>
      <textarea
        id={id}
        value={value}
        rows={rows ?? 3}
        placeholder={placeholder}
        onChange={(e) => onChange(e.currentTarget.value)}
        {...a11y}
      />
      {hint}
      <FieldError id={id} messages={messages} />
    </div>
  );
}

/** What an amount preview shows. */
interface Preview {
  /** The engine's reading of the text, e.g. `1½ cups`. */
  text: string;
  /** A warning (unknown unit, unreadable input). */
  warning: string | null;
}

/** The engine's live reading of a typed amount or duration. */
function usePreview(
  value: string,
  kind: "amount" | "duration",
): Preview | null {
  const { engine, form } = useEditor();
  const { t } = useTranslation("editorLists");
  return useMemo(() => {
    const typed = value.trim();
    if (!typed) return null;
    try {
      if (kind === "duration") {
        const q: Quantity = engine.parseDuration(typed);
        return { text: engine.formatQuantity(q, form.lang), warning: null };
      }
      const parsed = engine.parseAmount(typed, form.lang);
      const q = parsed.quantity;
      const text = engine.formatQuantity(q, form.lang);
      const warning =
        q.unit !== undefined && !parsed.unit_known
          ? t("amount.unknownUnit")
          : q.min === undefined
            ? t("amount.asText")
            : null;
      return { text, warning };
    } catch {
      return {
        text: "",
        warning:
          kind === "duration" ? t("amount.notDuration") : t("amount.notAmount"),
      };
    }
  }, [engine, form.lang, kind, value, t]);
}

/** A text input with the engine's live reading underneath. */
export function AmountField(
  props: TextFieldProps & { kind?: "amount" | "duration" },
) {
  const preview = usePreview(props.value, props.kind ?? "amount");
  const hint = preview ? (
    <span className="amount-preview" aria-live="polite">
      {preview.text && preview.text !== props.value.trim() ? (
        <span className="muted">= {preview.text}</span>
      ) : null}
      {preview.warning ? (
        <span className="amount-warning">{preview.warning}</span>
      ) : null}
    </span>
  ) : null;
  const { kind: _kind, ...rest } = props;
  return <TextField {...rest} hint={hint} />;
}

/** A labeled select. */
export function SelectField<T extends string>({
  id,
  label,
  value,
  options,
  onChange,
  hideLabel,
  className,
}: {
  id: string;
  label: string;
  value: T;
  options: readonly { value: T; label: string }[];
  onChange: (value: T) => void;
  hideLabel?: boolean;
  className?: string;
}) {
  const { messages, a11y } = useField(id);
  return (
    <div className={`field ${className ?? ""}`}>
      <label htmlFor={id}>
        <span className={hideLabel ? "visually-hidden" : undefined}>
          {label}
        </span>
      </label>
      <select
        id={id}
        value={value}
        onChange={(e) => {
          const next = options.find((o) => o.value === e.currentTarget.value);
          if (next) onChange(next.value);
        }}
        {...a11y}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <FieldError id={id} messages={messages} />
    </div>
  );
}

/** A checkbox with its label. */
export function CheckField({
  id,
  label,
  checked,
  onChange,
}: {
  id: string;
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="check" htmlFor={id}>
      <input
        id={id}
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.currentTarget.checked)}
      />
      <span>{label}</span>
    </label>
  );
}

/** Toggle chips over a fixed vocabulary (course, diet, allergens). */
export function ChipToggles({
  id,
  label,
  options,
  selected,
  onChange,
}: {
  id: string;
  label: string;
  options: readonly string[];
  selected: readonly string[];
  onChange: (next: string[]) => void;
}) {
  const { messages, a11y } = useField(id);
  const all = [...options, ...selected.filter((s) => !options.includes(s))];
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
        {all.map((code) => {
          const on = selected.includes(code);
          return (
            <button
              key={code}
              type="button"
              className="chip"
              aria-pressed={on}
              onClick={() =>
                onChange(
                  on ? selected.filter((s) => s !== code) : [...selected, code],
                )
              }
            >
              {code.replace(/-/g, " ")}
            </button>
          );
        })}
      </div>
      <FieldError id={id} messages={messages} />
    </fieldset>
  );
}

/** Free slugs (cuisine, tags): type and press Enter or comma; chips to remove. */
export function SlugField({
  id,
  label,
  values,
  onChange,
  placeholder,
}: {
  id: string;
  label: string;
  values: readonly string[];
  onChange: (next: string[]) => void;
  placeholder?: string;
}) {
  const { messages, a11y } = useField(id);
  const { t } = useTranslation("editorLists");
  const { maxSlugBytes } = useEditor().vocab;
  const [text, setText] = useState("");
  const commit = (typed: string) => {
    if (typed.trim()) onChange(addSlugs(values, typed, maxSlugBytes));
    setText("");
  };
  return (
    <div className="field">
      <label htmlFor={id}>
        <span>{label}</span>
      </label>
      {values.length > 0 ? (
        <ul className="chips clean slug-list">
          {values.map((slug) => (
            <li key={slug}>
              <button
                type="button"
                className="chip"
                aria-label={t("slug.remove", { slug })}
                onClick={() => onChange(values.filter((v) => v !== slug))}
              >
                {slug} <span aria-hidden="true">×</span>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      <input
        id={id}
        type="text"
        value={text}
        placeholder={placeholder}
        autoComplete="off"
        enterKeyHint="done"
        onChange={(e) => {
          const next = e.currentTarget.value;
          if (/[,;\n]/.test(next)) commit(next);
          else setText(next);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit(text);
          } else if (e.key === "Backspace" && !text && values.length) {
            onChange(values.slice(0, -1));
          }
        }}
        {...a11y}
        onBlur={() => {
          commit(text);
          a11y.onBlur();
        }}
      />
      <FieldError id={id} messages={messages} />
    </div>
  );
}

/** A small square icon button with an accessible name. */
export function IconButton({
  label,
  onClick,
  children,
  disabled,
  className,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      className={`icon ghost ${className ?? ""}`}
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
    >
      <span aria-hidden="true">{children}</span>
    </button>
  );
}
