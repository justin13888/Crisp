/**
 * A field's validation state: its problems and the ARIA wiring that points
 * assistive tech at them.
 */
import { useEditor } from "./context";

/** Accessibility wiring for one field. */
export interface FieldA11y {
  /** `aria-invalid`. */
  "aria-invalid": boolean | undefined;
  /** `aria-describedby`, when there is a message. */
  "aria-describedby": string | undefined;
  /** Marks the field visited. */
  onBlur: () => void;
}

/** Problems and ARIA props for a field id. */
export function useField(id: string): {
  messages: string[] | undefined;
  a11y: FieldA11y;
} {
  const { problems, touch } = useEditor();
  const messages = problems.get(id);
  return {
    messages,
    a11y: {
      "aria-invalid": messages ? true : undefined,
      "aria-describedby": messages ? `${id}-error` : undefined,
      onBlur: () => touch(id),
    },
  };
}

/** The messages under a field. */
export function FieldError({
  id,
  messages,
}: {
  id: string;
  messages: string[] | undefined;
}) {
  if (!messages?.length) return null;
  return (
    <p id={`${id}-error`} className="field-error">
      {messages.join(" ")}
    </p>
  );
}
