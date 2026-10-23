/**
 * Pieces shared by the ingredient, equipment, and step sections: row
 * toolbars, section headers, and the "paste a list" panel.
 */

import { type KeyboardEvent, useId, useState } from "react";
import {
  type Direction,
  type GroupKind,
  moveGroup,
  removeGroup,
  renameGroup,
} from "./actions";
import { useEditor } from "./context";
import { FieldError, IconButton, useField } from "./fields";
import type { GroupForm } from "./model";

/** Move up / move down / delete buttons for a row. */
export function RowActions({
  name,
  onMove,
  onDelete,
  first,
  last,
}: {
  name: string;
  onMove: (dir: Direction) => void;
  onDelete: () => void;
  first: boolean;
  last: boolean;
}) {
  return (
    <div className="row-actions">
      <IconButton
        label={`Move ${name} up`}
        onClick={() => onMove(-1)}
        disabled={first}
      >
        ↑
      </IconButton>
      <IconButton
        label={`Move ${name} down`}
        onClick={() => onMove(1)}
        disabled={last}
      >
        ↓
      </IconButton>
      <IconButton
        label={`Delete ${name}`}
        onClick={onDelete}
        className="delete"
      >
        ✕
      </IconButton>
    </div>
  );
}

/** Whether a keystroke should advance to (or create) the next row. */
export function isAdvanceKey(event: KeyboardEvent): boolean {
  return (
    event.key === "Enter" &&
    !event.shiftKey &&
    !event.altKey &&
    !event.ctrlKey &&
    !event.metaKey &&
    !event.nativeEvent.isComposing
  );
}

/** A section (group) header: editable title, reorder, delete. */
export function GroupHeader({
  kind,
  group,
  fieldId,
  first,
  last,
}: {
  kind: GroupKind;
  group: GroupForm;
  fieldId: string;
  first: boolean;
  last: boolean;
}) {
  const { update } = useEditor();
  const { messages, a11y } = useField(fieldId);
  const name = group.name.trim() || "untitled section";
  return (
    <div className="group-header">
      <div className="field">
        <label htmlFor={fieldId}>
          <span className="visually-hidden">Section title</span>
        </label>
        <input
          id={fieldId}
          type="text"
          className="group-title"
          value={group.name}
          placeholder={
            kind === "ingredient" ? "For the sauce" : "Make the dough"
          }
          autoComplete="off"
          onChange={(e) => {
            const value = e.currentTarget.value;
            update((f) => renameGroup(f, kind, group.key, value));
          }}
          {...a11y}
        />
        <FieldError id={fieldId} messages={messages} />
      </div>
      <div className="row-actions">
        <IconButton
          label={`Move section ${name} up`}
          onClick={() => update((f) => moveGroup(f, kind, group.key, -1))}
          disabled={first}
        >
          ↑
        </IconButton>
        <IconButton
          label={`Move section ${name} down`}
          onClick={() => update((f) => moveGroup(f, kind, group.key, 1))}
          disabled={last}
        >
          ↓
        </IconButton>
        <IconButton
          label={`Delete section ${name} (keeps its items)`}
          onClick={() => update((f) => removeGroup(f, kind, group.key))}
          className="delete"
        >
          ✕
        </IconButton>
      </div>
    </div>
  );
}

/** A disclosure with a textarea for pasting many lines at once. */
export function BulkPaste({
  title,
  help,
  placeholder,
  onApply,
}: {
  title: string;
  help: string;
  placeholder: string;
  onApply: (text: string) => void;
}) {
  const [text, setText] = useState("");
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <details
      className="bulk-paste"
      open={open}
      onToggle={(e) => setOpen(e.currentTarget.open)}
    >
      <summary>{title}</summary>
      <div className="stack-sm">
        <label htmlFor={id}>
          <span>{help}</span>
        </label>
        <textarea
          id={id}
          rows={6}
          value={text}
          placeholder={placeholder}
          onChange={(e) => setText(e.currentTarget.value)}
        />
        <div className="cluster">
          <button
            type="button"
            disabled={!text.trim()}
            onClick={() => {
              onApply(text);
              setText("");
              setOpen(false);
            }}
          >
            Add these
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => setOpen(false)}
          >
            Cancel
          </button>
        </div>
      </div>
    </details>
  );
}

/** Whether any visible problem belongs to a field id with one of these prefixes. */
export function hasProblems(
  problems: ReadonlyMap<string, string[]>,
  prefixes: readonly string[],
  except: readonly string[] = [],
): boolean {
  for (const key of problems.keys()) {
    if (except.includes(key)) continue;
    if (prefixes.some((prefix) => key.startsWith(prefix))) return true;
  }
  return false;
}
