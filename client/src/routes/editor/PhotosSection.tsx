import { useId, useState } from "react";
import { errorMessage } from "../../lib/crisp";
import { createLogger } from "../../lib/log";
import { moveMedia } from "./actions";
import { useEditor } from "./context";
import { FieldError, IconButton, useField } from "./fields";
import { prepareImage, useDraftBlobUrl } from "./images";
import { fieldId, LIMITS } from "./model";

const log = createLogger("editor.photos");

/** Adds picked image files to the draft's blobs; returns their ids. */
function useAddImages() {
  const { engine, update } = useEditor();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const add = async (files: readonly File[]): Promise<string[]> => {
    setBusy(true);
    setError(null);
    const ids: string[] = [];
    try {
      for (const file of files) {
        const image = await prepareImage(file, engine);
        update((f) => ({
          ...f,
          blobs: { ...f.blobs, [image.id]: image.blob },
        }));
        ids.push(image.id);
      }
    } catch (cause: unknown) {
      log.warn("image.failed", { error: errorMessage(cause) });
      setError(`Could not add that photo: ${errorMessage(cause)}`);
    } finally {
      setBusy(false);
    }
    return ids;
  };
  return { add, busy, error };
}

/** A thumbnail of a draft image. */
function Thumb({ blobId, alt }: { blobId: string; alt: string }) {
  const { form } = useEditor();
  const url = useDraftBlobUrl(form.blobs[blobId]);
  return url ? (
    <img className="thumb" src={url} alt={alt} />
  ) : (
    <span className="thumb thumb-missing">?</span>
  );
}

/** A single-image picker (step photos). */
export function ImagePicker({
  id,
  label,
  blobId,
  onPicked,
  onRemove,
}: {
  id: string;
  label: string;
  blobId: string | null;
  onPicked: (blobId: string) => void;
  onRemove: () => void;
}) {
  const { add, busy, error } = useAddImages();
  const { messages, a11y } = useField(id);
  return (
    <div className="field image-picker">
      <span className="field-label">{label}</span>
      <div className="cluster">
        {blobId ? <Thumb blobId={blobId} alt={label} /> : null}
        <label className="button secondary file-button" htmlFor={id}>
          {busy ? "Processing…" : blobId ? "Replace photo" : "Add photo"}
        </label>
        <input
          id={id}
          type="file"
          accept="image/*"
          className="visually-hidden"
          onChange={async (e) => {
            const file = e.currentTarget.files?.[0];
            e.currentTarget.value = "";
            if (!file) return;
            const [picked] = await add([file]);
            if (picked) onPicked(picked);
          }}
          {...a11y}
        />
        {blobId ? (
          <button type="button" className="ghost" onClick={onRemove}>
            Remove photo
          </button>
        ) : null}
      </div>
      {error ? <p className="field-error">{error}</p> : null}
      <FieldError id={id} messages={messages} />
    </div>
  );
}

/** Recipe photos; the first is the cover. */
export function PhotosSection() {
  const { form, update } = useEditor();
  const { add, busy, error } = useAddImages();
  const { messages, a11y } = useField(fieldId.media);
  const inputId = useId();
  return (
    <section
      id="sec-photos"
      className="editor-section"
      aria-labelledby="h-photos"
    >
      <h2 id="h-photos">Photos</h2>
      {form.media.length > 0 ? (
        <ol className="photo-grid clean">
          {form.media.map((id, i) => (
            <li key={id} className="photo">
              <Thumb
                blobId={id}
                alt={i === 0 ? "Cover photo" : `Photo ${i + 1}`}
              />
              {i === 0 ? <span className="badge ok">Cover</span> : null}
              <div className="row-actions">
                <IconButton
                  label={`Move photo ${i + 1} earlier`}
                  disabled={i === 0}
                  onClick={() => update((f) => moveMedia(f, id, -1))}
                >
                  ←
                </IconButton>
                <IconButton
                  label={`Move photo ${i + 1} later`}
                  disabled={i === form.media.length - 1}
                  onClick={() => update((f) => moveMedia(f, id, 1))}
                >
                  →
                </IconButton>
                <IconButton
                  label={`Remove photo ${i + 1}`}
                  className="delete"
                  onClick={() =>
                    update((f) => ({
                      ...f,
                      media: f.media.filter((m) => m !== id),
                    }))
                  }
                >
                  ✕
                </IconButton>
              </div>
            </li>
          ))}
        </ol>
      ) : (
        <p className="muted hint">The first photo is the cover.</p>
      )}
      <div className="cluster">
        <label
          className="button secondary file-button"
          htmlFor={fieldId.media}
          aria-disabled={busy || form.media.length >= LIMITS.media}
        >
          {busy ? "Processing…" : "+ Add photos"}
        </label>
        <input
          id={fieldId.media}
          type="file"
          accept="image/*"
          multiple
          className="visually-hidden"
          aria-labelledby={inputId}
          disabled={busy || form.media.length >= LIMITS.media}
          onChange={async (e) => {
            const files = [...(e.currentTarget.files ?? [])];
            e.currentTarget.value = "";
            const ids = await add(files);
            if (ids.length) {
              update((f) => ({
                ...f,
                media: [
                  ...f.media,
                  ...ids.filter((id) => !f.media.includes(id)),
                ].slice(0, LIMITS.media),
              }));
            }
          }}
          {...a11y}
        />
        <span id={inputId} className="visually-hidden">
          Add recipe photos
        </span>
      </div>
      {error ? <p className="field-error">{error}</p> : null}
      <FieldError id={fieldId.media} messages={messages} />
    </section>
  );
}
