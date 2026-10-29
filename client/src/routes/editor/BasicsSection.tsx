import { useTranslation } from "../../i18n";
import { useEditor } from "./context";
import { AmountField, TextArea, TextField } from "./fields";
import { fieldId, TIME_KEYS } from "./model";

/** Title, summary, yield, and times. */
export function BasicsSection() {
  const { t } = useTranslation("editor");
  const { form, update } = useEditor();
  return (
    <section
      id="sec-basics"
      className="editor-section"
      aria-labelledby="h-basics"
    >
      <h2 id="h-basics">{t("basics.heading")}</h2>
      <TextField
        id={fieldId.title}
        label={t("basics.title")}
        value={form.title}
        placeholder={t("basics.titlePlaceholder")}
        onChange={(title) => update((f) => ({ ...f, title }))}
        className="field-title"
      />
      <TextArea
        id={fieldId.summary}
        label={t("basics.summary")}
        value={form.summary}
        placeholder={t("basics.summaryPlaceholder")}
        onChange={(summary) => update((f) => ({ ...f, summary }))}
      />
      <div className="field-row">
        <AmountField
          id={fieldId.yieldAmount}
          label={t("basics.makes")}
          value={form.yieldAmount}
          placeholder="4"
          inputMode="decimal"
          onChange={(yieldAmount) => update((f) => ({ ...f, yieldAmount }))}
          className="field-narrow"
        />
        <TextField
          id={fieldId.yieldUnit}
          label={t("basics.unit")}
          value={form.yieldUnit}
          placeholder={t("basics.unitPlaceholder")}
          onChange={(yieldUnit) => update((f) => ({ ...f, yieldUnit }))}
        />
      </div>
      <fieldset className="times">
        <legend>{t("basics.times")}</legend>
        <div className="times-grid">
          {TIME_KEYS.map((key) => (
            <AmountField
              key={key}
              kind="duration"
              id={fieldId.time(key)}
              label={t(`basics.time.${key}`)}
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
