import { useTranslation } from "../../i18n";
import { useEditor } from "./context";
import {
  ChipToggles,
  SelectField,
  SlugField,
  TextArea,
  TextField,
} from "./fields";
import { fieldId, LICENSES } from "./model";

/** Course, diet, allergens, cuisine, and tags. */
export function ClassifySection() {
  const { t } = useTranslation("editor");
  const { form, update, vocab } = useEditor();
  return (
    <section id="sec-tags" className="editor-section" aria-labelledby="h-tags">
      <h2 id="h-tags">{t("classify.heading")}</h2>
      <ChipToggles
        id={fieldId.course}
        label={t("classify.course")}
        options={vocab.course}
        selected={form.course}
        onChange={(course) => update((f) => ({ ...f, course }))}
      />
      <ChipToggles
        id={fieldId.diet}
        label={t("classify.diet")}
        options={vocab.diet}
        selected={form.diet}
        onChange={(diet) => update((f) => ({ ...f, diet }))}
      />
      <ChipToggles
        id={fieldId.allergens}
        label={t("classify.allergens")}
        options={vocab.allergens}
        selected={form.allergens}
        onChange={(allergens) => update((f) => ({ ...f, allergens }))}
      />
      <SlugField
        id={fieldId.cuisine}
        label={t("classify.cuisine")}
        placeholder={t("classify.cuisinePlaceholder")}
        values={form.cuisine}
        onChange={(cuisine) => update((f) => ({ ...f, cuisine }))}
      />
      <SlugField
        id={fieldId.tags}
        label={t("classify.tags")}
        placeholder={t("classify.tagsPlaceholder")}
        values={form.tags}
        onChange={(tags) => update((f) => ({ ...f, tags }))}
      />
    </section>
  );
}

/** Notes, source, and license. */
export function NotesSection() {
  const { t } = useTranslation("editor");
  const { form, update } = useEditor();
  const offered: { value: string; label: string }[] = LICENSES.map((id) => ({
    value: id,
    label: t(`notes.license.${id}`),
  }));
  const licenseOptions = offered.some((o) => o.value === form.license)
    ? offered
    : [...offered, { value: form.license, label: form.license }];
  return (
    <section
      id="sec-notes"
      className="editor-section"
      aria-labelledby="h-notes"
    >
      <h2 id="h-notes">{t("notes.heading")}</h2>
      <TextArea
        id={fieldId.notes}
        label={t("notes.notes")}
        placeholder={t("notes.notesPlaceholder")}
        value={form.notes}
        onChange={(notes) => update((f) => ({ ...f, notes }))}
      />
      <fieldset className="source">
        <legend>{t("notes.source")}</legend>
        <div className="field-row">
          <TextField
            id={fieldId.source("title")}
            label={t("notes.sourceTitle")}
            value={form.source.title}
            onChange={(title) =>
              update((f) => ({ ...f, source: { ...f.source, title } }))
            }
          />
          <TextField
            id={fieldId.source("author")}
            label={t("notes.author")}
            value={form.source.author}
            onChange={(author) =>
              update((f) => ({ ...f, source: { ...f.source, author } }))
            }
          />
        </div>
        <TextField
          id={fieldId.source("url")}
          label={t("notes.url")}
          type="url"
          inputMode="url"
          placeholder="https://"
          value={form.source.url}
          onChange={(url) =>
            update((f) => ({ ...f, source: { ...f.source, url } }))
          }
        />
      </fieldset>
      <SelectField
        id={fieldId.license}
        label={t("notes.license")}
        value={form.license}
        options={licenseOptions}
        onChange={(license) => update((f) => ({ ...f, license }))}
      />
    </section>
  );
}
