import { type ReactNode, useState } from "react";
import type {
  HeatLevel,
  TemperatureTarget,
  TemperatureUnit,
} from "../../generated/crisp";
import {
  addBulkSteps,
  addGroup,
  addStep,
  duplicateStep,
  moveStep,
  regroup,
  removeStep,
  toggleStepEquipment,
  toggleStepIngredient,
  updateStep,
} from "./actions";
import { parseStepList } from "./bulk";
import { BulkPaste, GroupHeader, hasProblems, RowActions } from "./common";
import { useEditor } from "./context";
import {
  AmountField,
  CheckField,
  FieldError,
  IconButton,
  SelectField,
  TextArea,
  TextField,
  useField,
} from "./fields";
import { ImagePicker } from "./PhotosSection";
import {
  blankTimer,
  fieldId,
  HEAT_LEVELS,
  LIMITS,
  type StepForm,
  TEMPERATURE_TARGETS,
  TEMPERATURE_UNITS,
} from "./model";

/** A collapsible panel inside a step, open at first if it has content. */
function Panel({
  title,
  count,
  hasContent,
  flagged,
  children,
}: {
  title: string;
  count?: number;
  hasContent: boolean;
  flagged: boolean;
  children: ReactNode;
}) {
  const [initiallyOpen] = useState(hasContent);
  return (
    <details className="step-panel" open={initiallyOpen || undefined}>
      <summary>
        {title}
        {count ? <span className="badge">{count}</span> : null}
        {flagged ? <span className="badge bad">check</span> : null}
      </summary>
      <div className="stack-sm step-panel-body">{children}</div>
    </details>
  );
}

/** Which ingredients a step uses, each with an optional partial amount. */
function IngredientPicker({ step }: { step: StepForm }) {
  const { form, update } = useEditor();
  const id = fieldId.step(step.ref, "ingredients");
  const { messages, a11y } = useField(id);
  const named = form.ingredients.filter(
    (i) => i.name.trim() || i.amount.trim(),
  );
  if (named.length === 0) {
    return (
      <p className="muted hint">Add ingredients above to pick them here.</p>
    );
  }
  return (
    <fieldset
      id={id}
      tabIndex={-1}
      className="picker"
      aria-invalid={a11y["aria-invalid"]}
      aria-describedby={a11y["aria-describedby"]}
    >
      <legend className="visually-hidden">Ingredients used in this step</legend>
      <ul className="clean stack-sm">
        {named.map((ingredient) => {
          const j = step.ingredients.findIndex((p) => p.ref === ingredient.ref);
          const pick = j >= 0 ? step.ingredients[j] : undefined;
          const label = [ingredient.amount, ingredient.name]
            .filter(Boolean)
            .join(" ");
          return (
            <li key={ingredient.ref} className="picker-row">
              <CheckField
                id={`f-step-${step.ref}-use-${ingredient.ref}`}
                label={label}
                checked={Boolean(pick)}
                onChange={() =>
                  update((f) =>
                    toggleStepIngredient(f, step.ref, ingredient.ref),
                  )
                }
              />
              {pick ? (
                <AmountField
                  id={fieldId.stepPick(step.ref, j)}
                  label={`Portion of ${ingredient.name || "ingredient"} used here`}
                  hideLabel
                  placeholder="all (or e.g. half: 1/2 cup)"
                  value={pick.amount}
                  onChange={(amount) =>
                    update((f) =>
                      updateStep(f, step.ref, {
                        ingredients: step.ingredients.map((p, k) =>
                          k === j ? { ...p, amount } : p,
                        ),
                      }),
                    )
                  }
                  className="field-amount"
                />
              ) : null}
            </li>
          );
        })}
      </ul>
      <FieldError id={id} messages={messages} />
    </fieldset>
  );
}

/** Which equipment a step uses. */
function EquipmentPicker({ step }: { step: StepForm }) {
  const { form, update } = useEditor();
  const id = fieldId.step(step.ref, "equipment");
  const { messages } = useField(id);
  const named = form.equipment.filter((e) => e.name.trim());
  if (named.length === 0) {
    return <p className="muted hint">Add equipment above to pick it here.</p>;
  }
  return (
    <div id={id} tabIndex={-1} className="chips">
      {named.map((item) => (
        <button
          key={item.ref}
          type="button"
          className="chip"
          aria-pressed={step.equipment.includes(item.ref)}
          onClick={() =>
            update((f) => toggleStepEquipment(f, step.ref, item.ref))
          }
        >
          {item.name}
        </button>
      ))}
      <FieldError id={id} messages={messages} />
    </div>
  );
}

/** A step's timers. */
function Timers({ step }: { step: StepForm }) {
  const { update, focusLater } = useEditor();
  const set = (timers: StepForm["timers"]) =>
    update((f) => updateStep(f, step.ref, { timers }));
  return (
    <div className="stack-sm">
      {step.timers.map((timer, j) => (
        <div className="timer-row" key={j}>
          <AmountField
            kind="duration"
            id={fieldId.timer(step.ref, j, "duration")}
            label={`Timer ${j + 1} duration`}
            placeholder="20 min"
            value={timer.duration}
            onChange={(duration) =>
              set(step.timers.map((t, k) => (k === j ? { ...t, duration } : t)))
            }
            className="field-amount"
          />
          <TextField
            id={fieldId.timer(step.ref, j, "label")}
            label={`Timer ${j + 1} label`}
            placeholder="simmer"
            value={timer.label}
            onChange={(label) =>
              set(step.timers.map((t, k) => (k === j ? { ...t, label } : t)))
            }
          />
          <CheckField
            id={`f-step-${step.ref}-timer-${j}-passive`}
            label="Hands-off"
            checked={timer.passive}
            onChange={(passive) =>
              set(step.timers.map((t, k) => (k === j ? { ...t, passive } : t)))
            }
          />
          <IconButton
            label={`Remove timer ${j + 1}`}
            onClick={() => set(step.timers.filter((_, k) => k !== j))}
          >
            ✕
          </IconButton>
        </div>
      ))}
      {step.timers.length < LIMITS.timers ? (
        <button
          type="button"
          className="secondary"
          onClick={() => {
            set([...step.timers, blankTimer()]);
            focusLater(fieldId.timer(step.ref, step.timers.length, "duration"));
          }}
        >
          + Add timer
        </button>
      ) : null}
    </div>
  );
}

const HEAT_OPTIONS: { value: HeatLevel | ""; label: string }[] = [
  { value: "", label: "—" },
  ...HEAT_LEVELS.map((h) => ({ value: h, label: h.replace("-", " ") })),
];

const TARGET_OPTIONS: { value: TemperatureTarget | ""; label: string }[] = [
  { value: "", label: "—" },
  ...TEMPERATURE_TARGETS.map((t) => ({ value: t, label: t })),
];

/** Heat level and temperature. */
function Heat({ step }: { step: StepForm }) {
  const { update } = useEditor();
  const patch = (change: Partial<StepForm>) =>
    update((f) => updateStep(f, step.ref, change));
  const units = (Object.keys(TEMPERATURE_UNITS) as TemperatureUnit[])
    .filter((u) => u !== "K" || step.tempUnit === "K")
    .map((u) => ({ value: u, label: TEMPERATURE_UNITS[u] }));
  return (
    <div className="stack-sm">
      <SelectField
        id={fieldId.step(step.ref, "heat")}
        label="Heat"
        value={step.heat}
        options={HEAT_OPTIONS}
        onChange={(heat) => patch({ heat })}
      />
      <div className="temp-row">
        <TextField
          id={fieldId.step(step.ref, "temp-min")}
          label="Temperature"
          placeholder="180"
          inputMode="decimal"
          value={step.tempMin}
          onChange={(tempMin) => patch({ tempMin })}
        />
        <TextField
          id={fieldId.step(step.ref, "temp-max")}
          label="Up to (optional)"
          placeholder="200"
          inputMode="decimal"
          value={step.tempMax}
          onChange={(tempMax) => patch({ tempMax })}
        />
        <SelectField
          id={`f-step-${step.ref}-temp-unit`}
          label="Temperature unit"
          value={step.tempUnit}
          options={units}
          onChange={(tempUnit) => patch({ tempUnit })}
        />
        <SelectField
          id={fieldId.step(step.ref, "temp-target")}
          label="Of the"
          value={step.tempTarget}
          options={TARGET_OPTIONS}
          onChange={(tempTarget) => patch({ tempTarget })}
        />
      </div>
    </div>
  );
}

/** One step card. */
function StepCard({
  step,
  number,
  first,
  last,
}: {
  step: StepForm;
  number: number;
  first: boolean;
  last: boolean;
}) {
  const { form, update, create, focusLater, problems } = useEditor();
  const ref = step.ref;
  const patch = (change: Partial<StepForm>) =>
    update((f) => updateStep(f, ref, change));
  const prefix = `f-step-${ref}-`;
  const flagged = (...parts: string[]) =>
    hasProblems(
      problems,
      parts.map((part) => `${prefix}${part}`),
    );
  const groupOptions = [
    { value: "", label: "No section" },
    ...form.stepGroups.map((g, i) => ({
      value: String(g.key),
      label: g.name.trim() || `Section ${i + 1}`,
    })),
  ];
  return (
    <li className="step-card" data-ref={ref}>
      <div className="step-head">
        <span className="step-num" aria-hidden="true">
          {number}
        </span>
        <RowActions
          name={`step ${number}`}
          first={first}
          last={last}
          onMove={(dir) => update((f) => moveStep(f, ref, dir))}
          onDelete={() => update((f) => removeStep(f, ref))}
        />
      </div>
      <TextArea
        id={fieldId.step(ref, "text")}
        label={`Step ${number}`}
        hideLabel
        placeholder="What to do. Blank lines start new paragraphs."
        value={step.text}
        onChange={(text) => patch({ text })}
      />
      <Panel
        title="Ingredients used"
        count={step.ingredients.length}
        hasContent={step.ingredients.length > 0}
        flagged={flagged("pick-", "ingredients")}
      >
        <IngredientPicker step={step} />
      </Panel>
      {form.equipment.length > 0 ? (
        <Panel
          title="Equipment used"
          count={step.equipment.length}
          hasContent={step.equipment.length > 0}
          flagged={false}
        >
          <EquipmentPicker step={step} />
        </Panel>
      ) : null}
      <Panel
        title="Timers"
        count={step.timers.length}
        hasContent={step.timers.length > 0}
        flagged={flagged("timer-")}
      >
        <Timers step={step} />
      </Panel>
      <Panel
        title="Heat & temperature"
        hasContent={Boolean(step.heat || step.tempMin)}
        flagged={flagged("temp-", "heat")}
      >
        <Heat step={step} />
      </Panel>
      <Panel
        title="More"
        hasContent={Boolean(step.produces || step.note || step.image)}
        flagged={flagged("produces", "note", "image")}
      >
        <TextField
          id={fieldId.step(ref, "produces")}
          label="Makes (names the result)"
          placeholder="the dough"
          value={step.produces}
          onChange={(produces) => patch({ produces })}
        />
        <TextArea
          id={fieldId.step(ref, "note")}
          label="Tips"
          placeholder="Don't let the garlic brown."
          value={step.note}
          rows={2}
          onChange={(note) => patch({ note })}
        />
        <ImagePicker
          id={fieldId.step(ref, "image")}
          label="Step photo"
          blobId={step.image}
          onPicked={(id) => patch({ image: id })}
          onRemove={() => patch({ image: null })}
        />
        {form.stepGroups.length > 0 ? (
          <SelectField
            id={`f-step-${ref}-section`}
            label="Section"
            value={step.group === null ? "" : String(step.group)}
            options={groupOptions}
            onChange={(value) =>
              update((f) =>
                regroup(f, "step", ref, value ? Number(value) : null),
              )
            }
          />
        ) : null}
        <div className="cluster">
          <button
            type="button"
            className="secondary"
            onClick={() => {
              const created = create((f) => addStep(f, { after: ref }));
              focusLater(fieldId.step(created, "text"));
            }}
          >
            Insert step below
          </button>
          <button
            type="button"
            className="secondary"
            onClick={() => {
              const created = create((f) => duplicateStep(f, ref));
              focusLater(fieldId.step(created, "text"));
            }}
          >
            Duplicate
          </button>
        </div>
      </Panel>
    </li>
  );
}

/** The steps of one section (or the unsectioned ones). */
function Lane({
  group,
  steps,
  startNumber,
}: {
  group: number | null;
  steps: StepForm[];
  startNumber: number;
}) {
  const { create, focusLater } = useEditor();
  return (
    <>
      <ol className="step-list clean">
        {steps.map((step, i) => (
          <StepCard
            key={step.ref}
            step={step}
            number={startNumber + i}
            first={i === 0}
            last={i === steps.length - 1}
          />
        ))}
      </ol>
      <button
        type="button"
        className="secondary add-row"
        onClick={() => {
          const created = create((f) => addStep(f, { group }));
          focusLater(fieldId.step(created, "text"));
        }}
      >
        + Add step
      </button>
    </>
  );
}

/** The method, grouped into optional sections. */
export function StepsSection() {
  const { form, update, create, focusLater } = useEditor();
  const ungrouped = form.steps.filter((s) => s.group === null);
  let number = ungrouped.length + 1;
  return (
    <section
      id="sec-steps"
      className="editor-section"
      aria-labelledby="h-steps"
    >
      <h2 id="h-steps">Steps</h2>
      <div id={fieldId.steps} tabIndex={-1} className="anchor" />
      {ungrouped.length > 0 || form.stepGroups.length === 0 ? (
        <Lane group={null} steps={ungrouped} startNumber={1} />
      ) : null}
      {form.stepGroups.map((group, gi) => {
        const steps = form.steps.filter((s) => s.group === group.key);
        const start = number;
        number += steps.length;
        return (
          <div className="group" key={group.key}>
            <GroupHeader
              kind="step"
              group={group}
              fieldId={fieldId.stepGroup(group.key)}
              first={gi === 0}
              last={gi === form.stepGroups.length - 1}
            />
            <Lane group={group.key} steps={steps} startNumber={start} />
          </div>
        );
      })}
      <div className="cluster section-tools">
        <button
          type="button"
          className="secondary"
          disabled={form.stepGroups.length >= LIMITS.groups}
          onClick={() => {
            const created = create((f) => {
              const g = addGroup(f, "step");
              return {
                form: addStep(g.form, { group: g.ref }).form,
                ref: g.ref,
              };
            });
            focusLater(fieldId.stepGroup(created));
          }}
        >
          + Add section
        </button>
      </div>
      <BulkPaste
        title="Paste steps"
        help="Paste the method: blank lines separate steps (or one step per line)."
        placeholder={
          "Preheat the oven to 180 °C.\n\nCream the butter and sugar."
        }
        onApply={(text) => update((f) => addBulkSteps(f, parseStepList(text)))}
      />
    </section>
  );
}
