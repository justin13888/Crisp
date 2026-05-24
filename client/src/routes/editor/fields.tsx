/**
 * Shared form-field helpers for the recipe authoring form.
 *
 * `core::limits` counts string lengths in **UTF-8 bytes**, but HTML's
 * `maxLength` and JavaScript's `String.length` count UTF-16 code units. The
 * mismatch means `maxLength` is only a coarse upper bound for ASCII input;
 * the authoritative guard is [`byteLength`] paired with [`ByteCountedInput`]
 * / [`ByteCountedTextarea`], which show the live byte count and turn red the
 * instant it exceeds the limit.
 */

import type { ChangeEvent } from "react";
import type { Quantity, Rational } from "../../lib/types";

/** Returns the number of UTF-8 bytes a string occupies on the wire. */
export function byteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

/** A 128-bit random birth nonce, used to seed a recipe's frozen identity. */
export function birthNonce(): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(16));
}

/** Converts a free-text amount into a [`Quantity`], or `undefined` if blank.
 *
 * A numeric string becomes an exact rational (decimals are preserved without
 * loss); a non-numeric string becomes a localized text amount (e.g. "to taste").
 */
export function amountToQuantity(
  amount: string,
  unit: string,
  lang: string,
): Quantity | undefined {
  const trimmed = amount.trim();
  if (!trimmed) return undefined;
  const value = Number(trimmed);
  if (Number.isFinite(value)) {
    const decimals = trimmed.includes(".")
      ? (trimmed.split(".")[1]?.length ?? 0)
      : 0;
    const denominator = 10 ** decimals;
    const rational: Rational = [Math.round(value * denominator), denominator];
    const quantity: Quantity = { min: rational };
    if (unit.trim()) quantity.unit = unit.trim();
    return quantity;
  }
  return { text: { [lang]: trimmed } };
}

/** Props shared by the byte-counted text-input wrappers. */
interface ByteFieldProps {
  /** The current value. */
  value: string;
  /** Called with the next value when the user types. */
  onChange: (next: string) => void;
  /** The byte budget; the input turns red past this. */
  maxBytes: number;
  /** Whether the field accepts empty input. */
  required?: boolean;
  /** Placeholder displayed when the field is empty. */
  placeholder?: string;
  /** Inline CSS overrides — used for flex sizing in row layouts. */
  style?: React.CSSProperties;
  /** Optional error message displayed beneath the counter. */
  error?: string;
  /** Whether the field has been touched (controls whether `error` is shown). */
  touched?: boolean;
  /** Called when the field loses focus, used to mark it touched. */
  onBlur?: () => void;
}

/** Renders the `used/max` byte counter and any active error message. */
function FieldMeta({
  value,
  maxBytes,
  error,
  touched,
}: {
  value: string;
  maxBytes: number;
  error?: string;
  touched?: boolean;
}) {
  const used = byteLength(value);
  const over = used > maxBytes;
  return (
    <div
      style={{
        display: "flex",
        justifyContent: "space-between",
        fontSize: "0.78rem",
        marginTop: "0.2rem",
      }}
    >
      <span style={{ color: touched && error ? "var(--bad)" : "var(--muted)" }}>
        {touched && error ? error : " "}
      </span>
      <span style={{ color: over ? "var(--bad)" : "var(--muted)" }}>
        {used} / {maxBytes} B
      </span>
    </div>
  );
}

/** A single-line input that shows a live byte counter and caps `maxLength`. */
export function ByteCountedInput({
  value,
  onChange,
  maxBytes,
  required,
  placeholder,
  style,
  error,
  touched,
  onBlur,
}: ByteFieldProps) {
  return (
    <div style={style}>
      <input
        value={value}
        onChange={(e: ChangeEvent<HTMLInputElement>) =>
          onChange(e.target.value)
        }
        // `maxLength` counts UTF-16 units — strict enough for ASCII, loose
        // for multi-byte input; the byte counter is the real guard.
        maxLength={maxBytes}
        required={required}
        placeholder={placeholder}
        onBlur={onBlur}
        style={
          touched && (error || byteLength(value) > maxBytes)
            ? { borderColor: "var(--bad)" }
            : undefined
        }
      />
      <FieldMeta
        value={value}
        maxBytes={maxBytes}
        error={error}
        touched={touched}
      />
    </div>
  );
}

/** A multi-line textarea twin of [`ByteCountedInput`]. */
export function ByteCountedTextarea({
  value,
  onChange,
  maxBytes,
  required,
  placeholder,
  style,
  error,
  touched,
  onBlur,
}: ByteFieldProps) {
  return (
    <div style={style}>
      <textarea
        value={value}
        onChange={(e: ChangeEvent<HTMLTextAreaElement>) =>
          onChange(e.target.value)
        }
        maxLength={maxBytes}
        required={required}
        placeholder={placeholder}
        onBlur={onBlur}
        style={
          touched && (error || byteLength(value) > maxBytes)
            ? { borderColor: "var(--bad)" }
            : undefined
        }
      />
      <FieldMeta
        value={value}
        maxBytes={maxBytes}
        error={error}
        touched={touched}
      />
    </div>
  );
}

/**
 * Humanizes an enum wire code like `gluten-free` or `tree-nuts` for display.
 * The wire string itself is the stored value — this is presentation only.
 */
export function humanizeCode(code: string): string {
  return code
    .split(/[-_]/)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}
