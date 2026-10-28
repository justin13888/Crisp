/**
 * The editor's surrounding chrome: the section navigation, the header with
 * its autosave status, and the "restored your draft" banner.
 */

import { useTranslation } from "../../i18n";

/** Where the autosave stands. */
export type AutosaveStatus = "idle" | "pending" | "saved" | "error";

const SECTIONS = [
  { id: "sec-basics", label: "nav.basics" },
  { id: "sec-ingredients", label: "nav.ingredients" },
  { id: "sec-equipment", label: "nav.equipment" },
  { id: "sec-steps", label: "nav.steps" },
  { id: "sec-photos", label: "nav.photos" },
  { id: "sec-tags", label: "nav.tags" },
  { id: "sec-notes", label: "nav.notes" },
  { id: "sec-save", label: "nav.save" },
] as const;

/** Jump links to each editor section. */
export function SectionNav() {
  const { t } = useTranslation("editor");
  return (
    <nav className="editor-nav" aria-label={t("nav.label")}>
      <ul className="clean">
        {SECTIONS.map((s) => (
          <li key={s.id}>
            <a href={`#${s.id}`}>{t(s.label)}</a>
          </li>
        ))}
      </ul>
    </nav>
  );
}

const AUTOSAVE_LABELS = {
  saved: "autosave.saved",
  pending: "autosave.pending",
  error: "autosave.error",
} as const;

/** The page title and the live autosave status. */
export function EditorHeader({
  heading,
  autosave,
}: {
  heading: string;
  autosave: AutosaveStatus;
}) {
  const { t } = useTranslation("editor");
  return (
    <header className="editor-header spread">
      <h1>{heading}</h1>
      <span className="muted autosave" aria-live="polite">
        {autosave === "idle" ? "" : t(AUTOSAVE_LABELS[autosave])}
      </span>
    </header>
  );
}

/** Offers to discard an autosaved draft that was restored on open. */
export function RestoredBanner({
  onDiscard,
  onKeep,
}: {
  onDiscard: () => void;
  onKeep: () => void;
}) {
  const { t } = useTranslation("editor");
  return (
    <div className="banner warn spread" role="status">
      <span>{t("restored.message")}</span>
      <span className="cluster">
        <button type="button" className="secondary" onClick={onDiscard}>
          {t("restored.discard")}
        </button>
        <button type="button" className="ghost" onClick={onKeep}>
          {t("restored.keep")}
        </button>
      </span>
    </div>
  );
}
