/**
 * An ingredient (or substitute) name field linked to the ingredient index:
 * a name that matches one entry exactly attaches its slug as a removable
 * chip; otherwise, while the field has focus, ranked suggestions offer the
 * entries the author may pick. The typed name is never rewritten.
 */
import { type FocusEvent, type KeyboardEvent, useMemo, useState } from "react";
import { useTranslation } from "../../i18n";
import { createLogger } from "../../lib/log";
import { useEditor } from "./context";
import { TextField } from "./fields";
import { relink, suggestions } from "./linking";

const log = createLogger("editor");

/** A name and the index code it carries. */
export interface LinkedName {
  /** The name as the author typed it. */
  name: string;
  /** The linked slug, if any. */
  code: string | undefined;
}

/** Props of {@link LinkedNameField}. */
interface LinkedNameFieldProps {
  /** The input's id. */
  id: string;
  /** Its accessible label (visually hidden). */
  label: string;
  /** Placeholder text. */
  placeholder: string;
  /** The current name and code. */
  value: LinkedName;
  /** Receives the new name and code together. */
  onChange(value: LinkedName): void;
  /** Keyboard handling for the input (e.g. Enter advances). */
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
  /** The input's `enterkeyhint`. */
  enterKeyHint?: "next" | "done";
  /** The wrapper's grid class. */
  className?: string;
}

/** A name input with its index chip or suggestions beneath it. */
export function LinkedNameField({
  id,
  label,
  placeholder,
  value,
  onChange,
  onKeyDown,
  enterKeyHint,
  className,
}: LinkedNameFieldProps) {
  const { t } = useTranslation("editorLists");
  const { engine, form } = useEditor();
  const lang = form.lang;
  const [focused, setFocused] = useState(false);
  const { name, code } = value;

  const linkedName = useMemo(
    () => (code ? (engine.ingredientName(code, lang)?.name ?? code) : ""),
    [engine, code, lang],
  );
  const offered = useMemo(
    () => (code || !focused ? [] : suggestions(engine, name, lang)),
    [engine, code, focused, name, lang],
  );

  const rename = (next: string) => {
    const nextCode = relink(engine, lang, name, next, code);
    if (nextCode !== code) {
      log.debug("editor.ingredient_link", {
        field: id,
        from: code ?? null,
        to: nextCode ?? null,
        how: "typed",
      });
    }
    onChange({ name: next, code: nextCode });
  };
  const pick = (slug: string | undefined) => {
    log.debug("editor.ingredient_link", {
      field: id,
      from: code ?? null,
      to: slug ?? null,
      how: slug ? "picked" : "removed",
    });
    onChange({ name, code: slug });
  };
  const blur = (event: FocusEvent<HTMLFieldSetElement>) => {
    const to = event.relatedTarget;
    if (!(to instanceof Node && event.currentTarget.contains(to))) {
      setFocused(false);
    }
  };

  return (
    // One group: the name, its chip, and its suggestions share a focus
    // scope, so tabbing from the input to a suggestion keeps them open.
    <fieldset
      className={`linked-name ${className ?? ""}`}
      onFocus={() => setFocused(true)}
      onBlur={blur}
    >
      <legend className="visually-hidden">{label}</legend>
      <TextField
        id={id}
        label={label}
        hideLabel
        placeholder={placeholder}
        value={name}
        onChange={rename}
        onKeyDown={onKeyDown}
        enterKeyHint={enterKeyHint}
      />
      {code ? (
        <span
          className="chip ingredient-link"
          title={t("link.chipHint", { slug: code })}
          data-code={code}
        >
          {linkedName}
          <button
            type="button"
            className="ingredient-link__remove"
            aria-label={t("link.remove", { name: linkedName })}
            onClick={() => pick(undefined)}
          >
            ✕
          </button>
        </span>
      ) : null}
      {offered.length > 0 ? (
        <fieldset className="chips ingredient-suggestions">
          <legend className="visually-hidden">{t("link.suggestions")}</legend>
          {offered.map((s) => (
            <button
              key={s.slug}
              type="button"
              className="chip"
              title={s.slug}
              data-code={s.slug}
              onClick={() => pick(s.slug)}
            >
              {s.name}
            </button>
          ))}
        </fieldset>
      ) : null}
    </fieldset>
  );
}
