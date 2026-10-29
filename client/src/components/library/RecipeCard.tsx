import { useEffect, useId, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useTranslation } from "../../i18n";
import type { LibraryEntry } from "../../lib/library";
import { useLazyCover } from "./covers";
import { entryTitle, relativeTime } from "./libraryModel";
import { TrustBadge } from "./ui";

/** Props of {@link RecipeCard}. */
export interface RecipeCardProps {
  /** The stored, verified recipe. */
  entry: LibraryEntry;
  /** "Now", for relative dates (shared so the grid re-renders together). */
  now: number;
  /** Shares or downloads the file. */
  onExport(entry: LibraryEntry): void;
  /** Asks to delete the file. */
  onDelete(entry: LibraryEntry): void;
}

/** A small overflow menu (export / delete) for one card. */
function CardMenu({
  title,
  onExport,
  onDelete,
}: {
  title: string;
  onExport(): void;
  onDelete(): void;
}) {
  const { t } = useTranslation("library");
  const [open, setOpen] = useState(false);
  const menuId = useId();
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: Event) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const choose = (action: () => void) => () => {
    setOpen(false);
    action();
  };

  return (
    <div className="card-menu" ref={root}>
      <button
        type="button"
        className="ghost icon card-menu__toggle"
        aria-label={t("card.moreActions", { title })}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={() => setOpen((v) => !v)}
      >
        ⋮
      </button>
      {open && (
        <div className="card-menu__list" id={menuId} role="menu">
          <button
            type="button"
            role="menuitem"
            className="ghost"
            onClick={choose(onExport)}
          >
            {t("card.export")}
          </button>
          <button
            type="button"
            role="menuitem"
            className="ghost card-menu__danger"
            onClick={choose(onDelete)}
          >
            {t("card.delete")}
          </button>
        </div>
      )}
    </div>
  );
}

/** One recipe in the library grid: cover, title, trust, last change. */
export function RecipeCard({
  entry,
  now,
  onExport,
  onDelete,
}: RecipeCardProps) {
  const { t } = useTranslation("library");
  const [element, setElement] = useState<HTMLElement | null>(null);
  const title = entryTitle(entry);
  const cover = useLazyCover(element, {
    docId: entry.record.docId,
    revision: entry.record.revision,
    bytes: entry.record.bytes,
    coverId: entry.summary.recipes[0]?.cover,
  });
  const modified = entry.summary.modified_at * 1000;
  const initial = title.trim().charAt(0).toUpperCase() || "?";

  return (
    <article
      className="recipe-card"
      ref={setElement}
      data-doc-id={entry.record.docId}
    >
      <Link
        to={`/recipe/${encodeURIComponent(entry.record.docId)}`}
        className="recipe-card__link"
      >
        <div className="recipe-card__cover">
          {cover ? (
            <img src={cover} alt="" loading="lazy" decoding="async" />
          ) : (
            <span className="recipe-card__initial" aria-hidden="true">
              {initial}
            </span>
          )}
        </div>
        <div className="recipe-card__body">
          <h3 className="recipe-card__title">{title}</h3>
          {entry.summary.authors.length > 0 && (
            <p className="recipe-card__authors">
              {entry.summary.authors.join(", ")}
            </p>
          )}
          <time
            className="recipe-card__time"
            dateTime={new Date(modified).toISOString()}
            title={t("card.modified", {
              date: new Date(modified).toLocaleString(),
            })}
          >
            {relativeTime(modified, now)}
          </time>
        </div>
      </Link>
      <div className="recipe-card__footer">
        <TrustBadge trust={entry.report.trust} />
        <CardMenu
          title={title}
          onExport={() => onExport(entry)}
          onDelete={() => onDelete(entry)}
        />
      </div>
    </article>
  );
}

/** A placeholder card with the same footprint, shown while loading. */
export function RecipeCardSkeleton() {
  return (
    <div className="recipe-card recipe-card--skeleton" aria-hidden="true">
      <div className="recipe-card__cover skeleton" />
      <div className="recipe-card__body">
        <div className="skeleton skeleton-line" />
        <div className="skeleton skeleton-line skeleton-line--short" />
        <div className="skeleton skeleton-line skeleton-line--tiny" />
      </div>
      <div className="recipe-card__footer">
        <div className="skeleton skeleton-pill" />
      </div>
    </div>
  );
}
