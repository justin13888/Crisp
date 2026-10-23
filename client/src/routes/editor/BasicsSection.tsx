import { useEditor } from "./context";
import { AmountField, TextArea, TextField } from "./fields";
import { fieldId, TIME_KEYS, TIME_LABELS } from "./model";

/** Title, summary, yield, and times. */
export function BasicsSection() {
  const { form, update } = useEditor();
  return (
    <section
      id="sec-basics"
      className="editor-section"
      aria-labelledby="h-basics"
    >
      <h2 id="h-basics">Basics</h2>
      <TextField
        id={fieldId.title}
        label="Title"
        value={form.title}
        placeholder="Grandma's tomato soup"
        onChange={(title) => update((f) => ({ ...f, title }))}
        className="field-title"
      />
      <TextArea
        id={fieldId.summary}
        label="Summary"
        value={form.summary}
        placeholder="A line or two about this recipe. Blank lines start new paragraphs."
        onChange={(summary) => update((f) => ({ ...f, summary }))}
      />
      <div className="field-row">
        <AmountField
          id={fieldId.yieldAmount}
          label="Makes"
          value={form.yieldAmount}
          placeholder="4"
          inputMode="decimal"
          onChange={(yieldAmount) => update((f) => ({ ...f, yieldAmount }))}
          className="field-narrow"
        />
        <TextField
          id={fieldId.yieldUnit}
          label="Unit"
          value={form.yieldUnit}
          placeholder="servings"
          onChange={(yieldUnit) => update((f) => ({ ...f, yieldUnit }))}
        />
      </div>
      <fieldset className="times">
        <legend>Times</legend>
        <div className="times-grid">
          {TIME_KEYS.map((key) => (
            <AmountField
              key={key}
              kind="duration"
              id={fieldId.time(key)}
              label={TIME_LABELS[key]}
              value={form.times[key]}
              placeholder={key === "total" ? "1h 15m" : "20 min"}
              onChange={(value) =>
                update((f) => ({ ...f, times: { ...f.times, [key]: value } }))
              }
            />
          ))}
        </div>
      </fieldset>
    </section>
  );
}
