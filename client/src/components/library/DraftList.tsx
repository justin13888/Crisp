import { Link } from "react-router-dom";
import type { DraftRecord } from "../../lib/store";
import { draftLink, draftTitle, relativeTime } from "./libraryModel";

/** Props of {@link DraftList}. */
export interface DraftListProps {
  /** Unsaved editor drafts, newest first. */
  drafts: DraftRecord[];
  /** "Now", for relative dates. */
  now: number;
  /** Asks to discard a draft. */
  onDiscard(draft: DraftRecord): void;
}

/** Unsaved editor drafts, each resumable or discardable. */
export function DraftList({ drafts, now, onDiscard }: DraftListProps) {
  const resumable = drafts.flatMap((draft) => {
    const to = draftLink(draft.id);
    return to ? [{ draft, to }] : [];
  });
  if (resumable.length === 0) return null;
  return (
    <section className="drafts" aria-labelledby="drafts-heading">
      <h2 id="drafts-heading" className="section-title">
        Unsaved drafts
      </h2>
      <ul className="clean drafts__list">
        {resumable.map(({ draft, to }) => {
          const title = draftTitle(draft.state, draft.id);
          return (
            <li key={draft.id} className="draft-row">
              <Link to={to} className="draft-row__link">
                <span className="draft-row__title">{title}</span>
                <span className="muted draft-row__meta">
                  {draft.id === "new" ? "New recipe" : "Edits"} · saved{" "}
                  {relativeTime(draft.updatedAt, now)}
                </span>
              </Link>
              <button
                type="button"
                className="ghost"
                aria-label={`Discard draft ${title}`}
                onClick={() => onDiscard(draft)}
              >
                Discard
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
