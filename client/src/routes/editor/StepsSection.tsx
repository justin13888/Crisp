import { useTranslation } from "../../i18n";
import {
  addBulkSteps,
  addGroup,
  addStep,
  duplicateStep,
  moveStep,
  regroup,
  removeStep,
  updateStep,
} from "./actions";
import { parseStepList } from "./bulk";
import { BulkPaste, GroupHeader, hasProblems, RowActions } from "./common";
import { useEditor } from "./context";
import { SelectField, TextArea, TextField } from "./fields";
import { fieldId, LIMITS, type StepForm } from "./model";
import { ImagePicker } from "./PhotosSection";
import { EquipmentPicker, IngredientPicker, Panel } from "./StepPickers";
import { Heat, Timers } from "./StepTiming";

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
  const { t } = useTranslation("editorLists");
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
    { value: "", label: t("section.none") },
    ...form.stepGroups.map((g, i) => ({
      value: String(g.key),
      label: g.name.trim() || t("section.numbered", { number: i + 1 }),
    })),
  ];
  return (
    <li className="step-card" data-ref={ref}>
      <div className="step-head">
        <span className="step-num" aria-hidden="true">
          {number}
        </span>
        <RowActions
          name={t("step.rowName", { number })}
          first={first}
          last={last}
          onMove={(dir) => update((f) => moveStep(f, ref, dir))}
          onDelete={() => update((f) => removeStep(f, ref))}
        />
      </div>
      <TextArea
        id={fieldId.step(ref, "text")}
        label={t("step.text", { number })}
        hideLabel
        placeholder={t("step.textPlaceholder")}
        value={step.text}
        onChange={(text) => patch({ text })}
      />
      <Panel
        title={t("step.panel.ingredients")}
        count={step.ingredients.length}
        hasContent={step.ingredients.length > 0}
        flagged={flagged("pick-", "ingredients")}
      >
        <IngredientPicker step={step} />
      </Panel>
      {form.equipment.length > 0 ? (
        <Panel
          title={t("step.panel.equipment")}
          count={step.equipment.length}
          hasContent={step.equipment.length > 0}
          flagged={false}
        >
          <EquipmentPicker step={step} />
        </Panel>
      ) : null}
      <Panel
        title={t("step.panel.timers")}
        count={step.timers.length}
        hasContent={step.timers.length > 0}
        flagged={flagged("timer-")}
      >
        <Timers step={step} />
      </Panel>
      <Panel
        title={t("step.panel.heat")}
        hasContent={Boolean(step.heat || step.tempMin)}
        flagged={flagged("temp-", "heat")}
      >
        <Heat step={step} />
      </Panel>
      <Panel
        title={t("row.more")}
        hasContent={Boolean(step.produces || step.note || step.image)}
        flagged={flagged("produces", "note", "image")}
      >
        <TextField
          id={fieldId.step(ref, "produces")}
          label={t("step.produces")}
          placeholder={t("step.producesPlaceholder")}
          value={step.produces}
          onChange={(produces) => patch({ produces })}
        />
        <TextArea
          id={fieldId.step(ref, "note")}
          label={t("step.note")}
          placeholder={t("step.notePlaceholder")}
          value={step.note}
          rows={2}
          onChange={(note) => patch({ note })}
        />
        <ImagePicker
          id={fieldId.step(ref, "image")}
          label={t("step.photo")}
          blobId={step.image}
          onPicked={(id) => patch({ image: id })}
          onRemove={() => patch({ image: null })}
        />
        {form.stepGroups.length > 0 ? (
          <SelectField
            id={`f-step-${ref}-section`}
            label={t("section.label")}
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
            {t("step.insertBelow")}
          </button>
          <button
            type="button"
            className="secondary"
            onClick={() => {
              const created = create((f) => duplicateStep(f, ref));
              focusLater(fieldId.step(created, "text"));
            }}
          >
            {t("row.duplicate")}
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
  const { t } = useTranslation("editorLists");
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
        {t("steps.add")}
      </button>
    </>
  );
}

/** The method, grouped into optional sections. */
export function StepsSection() {
  const { t } = useTranslation("editorLists");
  const { form, update, create, focusLater } = useEditor();
  const ungrouped = form.steps.filter((s) => s.group === null);
  let number = ungrouped.length + 1;
  return (
    <section
      id="sec-steps"
      className="editor-section"
      aria-labelledby="h-steps"
    >
      <h2 id="h-steps">{t("steps.heading")}</h2>
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
          {t("section.add")}
        </button>
      </div>
      <BulkPaste
        title={t("steps.paste.title")}
        help={t("steps.paste.help")}
        placeholder={t("steps.paste.placeholder")}
        onApply={(text) => update((f) => addBulkSteps(f, parseStepList(text)))}
      />
    </section>
  );
}
