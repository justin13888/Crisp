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
  const { form, update, vocab } = useEditor();
  return (
    <section id="sec-tags" className="editor-section" aria-labelledby="h-tags">
      <h2 id="h-tags">Course, diet & tags</h2>
      <ChipToggles
        id={fieldId.course}
        label="Course"
        options={vocab.course}
        selected={form.course}
        onChange={(course) => update((f) => ({ ...f, course }))}
      />
      <ChipToggles
        id={fieldId.diet}
        label="Diet"
        options={vocab.diet}
        selected={form.diet}
        onChange={(diet) => update((f) => ({ ...f, diet }))}
      />
      <ChipToggles
        id={fieldId.allergens}
        label="Contains allergens"
        options={vocab.allergens}
        selected={form.allergens}
        onChange={(allergens) => update((f) => ({ ...f, allergens }))}
      />
      <SlugField
        id={fieldId.cuisine}
        label="Cuisine"
        placeholder="italian, sichuan…"
        values={form.cuisine}
        onChange={(cuisine) => update((f) => ({ ...f, cuisine }))}
      />
      <SlugField
        id={fieldId.tags}
        label="Tags"
        placeholder="weeknight, one-pot…"
        values={form.tags}
        onChange={(tags) => update((f) => ({ ...f, tags }))}
      />
    </section>
  );
}

const LICENSE_OPTIONS = LICENSES.map((l) => ({ value: l.id, label: l.label }));

/** Notes, source, and license. */
export function NotesSection() {
  const { form, update } = useEditor();
  const licenseOptions = LICENSE_OPTIONS.some((o) => o.value === form.license)
    ? LICENSE_OPTIONS
    : [...LICENSE_OPTIONS, { value: form.license, label: form.license }];
  return (
    <section
      id="sec-notes"
      className="editor-section"
      aria-labelledby="h-notes"
    >
      <h2 id="h-notes">Notes & source</h2>
      <TextArea
        id={fieldId.notes}
        label="Notes"
        placeholder="Storage, variations, serving ideas."
        value={form.notes}
        onChange={(notes) => update((f) => ({ ...f, notes }))}
      />
      <fieldset className="source">
        <legend>Source</legend>
        <div className="field-row">
          <TextField
            id={fieldId.source("title")}
            label="Book or site"
            value={form.source.title}
            onChange={(title) =>
              update((f) => ({ ...f, source: { ...f.source, title } }))
            }
          />
          <TextField
            id={fieldId.source("author")}
            label="Author"
            value={form.source.author}
            onChange={(author) =>
              update((f) => ({ ...f, source: { ...f.source, author } }))
            }
          />
        </div>
        <TextField
          id={fieldId.source("url")}
          label="Link (https)"
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
        label="License"
        value={form.license}
        options={licenseOptions}
        onChange={(license) => update((f) => ({ ...f, license }))}
      />
    </section>
  );
}
