import { Link } from "react-router-dom";
import { useTranslation } from "../../i18n";
import type { DraftRecord } from "../../lib/store";
import { describeDraft } from "../../routes/editor/drafts";
import { relativeTime } from "./libraryModel";

/** Props of {@link DraftList}. */
export interface DraftListProps {
  /** Unsaved editor drafts, newest first. */
  drafts: DraftRecord[];
  /** "Now", for relative dates. */
  now: number;
  /** Asks to discard a draft. */
  onDiscard(draft: DraftRecord): void;
}

/**
 * Unsaved editor drafts, each resumable or discardable. Drafts the editor
 * cannot read back are not listed.
 */
export function DraftList({ drafts, now, onDiscard }: DraftListProps) {
  const { t } = useTranslation("library");
  const resumable = drafts.flatMap((draft) => {
    const summary = describeDraft(draft);
    return summary ? [{ draft, summary }] : [];
  });
  if (resumable.length === 0) return null;
  return (
    <section className="drafts" aria-labelledby="drafts-heading">
      <h2 id="drafts-heading" className="section-title">
        {t("drafts.title")}
      </h2>
      <ul className="clean drafts__list">
        {resumable.map(({ draft, summary }) => {
          const kind = summary.editing ? t("drafts.edits") : t("drafts.new");
          const title =
            summary.title ||
            (summary.editing ? t("drafts.edited") : t("drafts.new"));
          return (
            <li key={summary.id} className="draft-row">
              <Link to={summary.href} className="draft-row__link">
                <span className="draft-row__title">{title}</span>
                <span className="muted draft-row__meta">
                  {t("drafts.meta", {
                    kind,
                    when: relativeTime(summary.updatedAt, now),
                  })}
                </span>
              </Link>
              <button
                type="button"
                className="ghost"
                aria-label={t("drafts.discardLabel", { title })}
                onClick={() => onDiscard(draft)}
              >
                {t("drafts.discard")}
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
