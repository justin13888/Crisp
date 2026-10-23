import type { KeyboardEvent } from "react";
import {
  addEquipment,
  duplicateEquipment,
  moveEquipment,
  removeEquipment,
  updateEquipment,
} from "./actions";
import { isAdvanceKey, RowActions } from "./common";
import { focusField, useEditor } from "./context";
import { AmountField, IconButton, TextField } from "./fields";
import { type EquipmentForm, fieldId } from "./model";

function EquipmentRow({
  item,
  number,
  first,
  last,
  next,
}: {
  item: EquipmentForm;
  number: number;
  first: boolean;
  last: boolean;
  next: EquipmentForm | undefined;
}) {
  const { update, create, focusLater } = useEditor();
  const ref = item.ref;
  const patch = (change: Partial<EquipmentForm>) =>
    update((f) => updateEquipment(f, ref, change));
  const advance = (event: KeyboardEvent<HTMLInputElement>) => {
    if (!isAdvanceKey(event)) return;
    event.preventDefault();
    if (next) {
      focusField(fieldId.equipment(next.ref, "name"));
      return;
    }
    const created = create((f) => addEquipment(f, { after: ref }));
    focusLater(fieldId.equipment(created, "name"));
  };
  const name = item.name.trim() || `equipment ${number}`;
  return (
    <li className="item-row">
      <div className="item-main equipment-main">
        <AmountField
          id={fieldId.equipment(ref, "quantity")}
          label={`How many of equipment ${number}`}
          hideLabel
          placeholder="1"
          value={item.quantity}
          onChange={(quantity) => patch({ quantity })}
          onKeyDown={advance}
          className="field-amount"
        />
        <TextField
          id={fieldId.equipment(ref, "name")}
          label={`Equipment ${number}`}
          hideLabel
          placeholder="baking sheet"
          value={item.name}
          onChange={(value) => patch({ name: value })}
          onKeyDown={advance}
          className="field-name"
        />
        <TextField
          id={fieldId.equipment(ref, "note")}
          label={`Note for equipment ${number}`}
          hideLabel
          placeholder="9×13 inch"
          value={item.note}
          onChange={(note) => patch({ note })}
          onKeyDown={advance}
          className="field-prep"
        />
        <RowActions
          name={name}
          first={first}
          last={last}
          onMove={(dir) => update((f) => moveEquipment(f, ref, dir))}
          onDelete={() => update((f) => removeEquipment(f, ref))}
        />
        <IconButton
          label={`Duplicate ${name}`}
          onClick={() => {
            const created = create((f) => duplicateEquipment(f, ref));
            focusLater(fieldId.equipment(created, "name"));
          }}
        >
          ⧉
        </IconButton>
      </div>
    </li>
  );
}

/** Pots, pans, and tools. */
export function EquipmentSection() {
  const { form, create, focusLater } = useEditor();
  return (
    <section
      id="sec-equipment"
      className="editor-section"
      aria-labelledby="h-equipment"
    >
      <h2 id="h-equipment">Equipment</h2>
      <div id={fieldId.equipmentList} tabIndex={-1} className="anchor" />
      {form.equipment.length === 0 ? (
        <p className="muted hint">Optional — list special pans or tools.</p>
      ) : (
        <ol className="item-list clean">
          {form.equipment.map((item, i) => (
            <EquipmentRow
              key={item.ref}
              item={item}
              number={i + 1}
              first={i === 0}
              last={i === form.equipment.length - 1}
              next={form.equipment[i + 1]}
            />
          ))}
        </ol>
      )}
      <button
        type="button"
        className="secondary add-row"
        onClick={() => {
          const created = create((f) => addEquipment(f));
          focusLater(fieldId.equipment(created, "name"));
        }}
      >
        + Add equipment
      </button>
    </section>
  );
}
