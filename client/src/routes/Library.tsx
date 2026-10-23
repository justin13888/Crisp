import "./library.css";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { DraftList } from "../components/library/DraftList";
import {
  DropOverlay,
  readSources,
  useFilePicker,
  useWindowFileDrop,
} from "../components/library/fileIntake";
import { ImportResults } from "../components/library/ImportResults";
import {
  entryTitle,
  filterEntries,
  isLibrarySort,
  type LibrarySort,
  removedFromState,
  SORT_OPTIONS,
  sortEntries,
} from "../components/library/libraryModel";
import {
  RecipeCard,
  RecipeCardSkeleton,
} from "../components/library/RecipeCard";
import { UndoToast } from "../components/library/UndoToast";
import { ConfirmDialog, Spinner } from "../components/library/ui";
import { useImportBatch } from "../components/library/useImportBatch";
import { errorMessage } from "../lib/crisp";
import {
  exportFile,
  fileName,
  type LibraryEntry,
  listEntries,
  removeEntry,
  restoreEntry,
} from "../lib/library";
import { createLogger } from "../lib/log";
import { type BundleRecord, type DraftRecord, getStore } from "../lib/store";

const log = createLogger("library.screen");

const SORT_KEY = "crisp.library.sort";
const SKELETONS = 6;

function storedSort(): LibrarySort {
  try {
    const value = localStorage.getItem(SORT_KEY);
    return isLibrarySort(value) ? value : "recent";
  } catch {
    return "recent";
  }
}

/** What the undo toast offers to put back. */
interface UndoOffer {
  record: BundleRecord;
  token: number;
}

/** What is pending confirmation. */
type Pending =
  | { kind: "delete"; entry: LibraryEntry }
  | { kind: "discard"; draft: DraftRecord };

/** The home screen: every recipe on this device. */
export function Library() {
  const location = useLocation();
  const navigate = useNavigate();
  const [entries, setEntries] = useState<LibraryEntry[] | null>(null);
  const [drafts, setDrafts] = useState<DraftRecord[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<LibrarySort>(storedSort);
  const [pending, setPending] = useState<Pending | null>(null);
  const [undo, setUndo] = useState<UndoOffer | null>(null);
  const [exporting, setExporting] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const batch = useImportBatch();
  const runBatch = batch.run;

  const reload = useCallback(async () => {
    const started = performance.now();
    try {
      const [list, store] = await Promise.all([listEntries(), getStore()]);
      setEntries(list);
      setDrafts(await store.drafts.all());
      setNow(Date.now());
      setLoadError(null);
      log.debug("loaded", {
        count: list.length,
        ms: Math.round(performance.now() - started),
      });
    } catch (error: unknown) {
      log.error("load.failed", { error: errorMessage(error) });
      setLoadError(errorMessage(error));
      setEntries((previous) => previous ?? []);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  // The recipe page navigates here after a delete with the removed record.
  useEffect(() => {
    const removed = removedFromState(location.state);
    if (!removed) return;
    setUndo({ record: removed, token: Date.now() });
    navigate(".", { replace: true, state: null });
  }, [location.state, navigate]);

  const importFiles = useCallback(
    async (files: File[]) => {
      const outcomes = await runBatch(await readSources(files));
      if (outcomes.some((o) => o.status !== "failed")) await reload();
    },
    [runBatch, reload],
  );
  const picker = useFilePicker((files) => void importFiles(files));
  const dragging = useWindowFileDrop((files) => void importFiles(files));

  const visible = useMemo(
    () => (entries ? sortEntries(filterEntries(entries, query), sort) : []),
    [entries, query, sort],
  );

  const changeSort = (value: string) => {
    if (!isLibrarySort(value)) return;
    setSort(value);
    try {
      localStorage.setItem(SORT_KEY, value);
    } catch {
      // Private mode: the choice just isn't remembered.
    }
  };

  const exportOne = async (entry: LibraryEntry) => {
    try {
      await exportFile(entry.record.bytes, fileName(entryTitle(entry)));
    } catch (error: unknown) {
      log.warn("export.failed", {
        docId: entry.record.docId,
        error: errorMessage(error),
      });
    }
  };

  const exportAll = async () => {
    if (!entries?.length) return;
    log.info("export.all", { count: entries.length });
    for (const [index, entry] of entries.entries()) {
      setExporting(`Exporting ${index + 1} of ${entries.length}…`);
      const result = await exportFile(
        entry.record.bytes,
        fileName(entryTitle(entry)),
      ).catch((error: unknown) => {
        log.warn("export.failed", { error: errorMessage(error) });
        return "failed" as const;
      });
      if (result === "cancelled") break;
    }
    setExporting(null);
  };

  const confirmPending = async () => {
    const current = pending;
    setPending(null);
    if (!current) return;
    if (current.kind === "delete") {
      const { docId } = current.entry.record;
      const removed = await removeEntry(docId);
      setEntries(
        (list) => list?.filter((e) => e.record.docId !== docId) ?? null,
      );
      if (removed) setUndo({ record: removed, token: Date.now() });
    } else {
      const store = await getStore();
      await store.drafts.delete(current.draft.id);
      log.info("draft.discarded", { id: current.draft.id });
      setDrafts((list) => list.filter((d) => d.id !== current.draft.id));
    }
  };

  const undoRemove = async () => {
    const offer = undo;
    setUndo(null);
    if (!offer) return;
    await restoreEntry(offer.record);
    await reload();
  };

  const loading = entries === null;
  const empty = !loading && entries.length === 0 && drafts.length === 0;

  return (
    <div className="library">
      {picker.input}
      <DropOverlay visible={dragging} />

      <header className="library__header">
        <div className="library__heading">
          <h1>Library</h1>
          {entries && entries.length > 0 && (
            <span className="muted tabular">
              {entries.length} recipe{entries.length === 1 ? "" : "s"}
            </span>
          )}
        </div>
        <div className="library__actions">
          <Link to="/new" className="button lg">
            <span aria-hidden="true">+</span> New recipe
          </Link>
          <button type="button" className="secondary lg" onClick={picker.open}>
            Import
          </button>
        </div>
      </header>

      {loadError && (
        <p className="banner bad" role="alert">
          Couldn’t read the library: {loadError}
        </p>
      )}

      <ImportResults rows={batch.rows} onDismiss={batch.clear} />

      {empty ? (
        <section className="library-empty card" aria-labelledby="empty-title">
          <div className="library-empty__art" aria-hidden="true">
            🍳
          </div>
          <h2 id="empty-title">Your cookbook starts here</h2>
          <p>
            Crisp keeps every recipe as one signed <code>.crisp</code> file on
            this device — it works offline and nothing leaves unless you share
            it.
          </p>
          <p className="muted">
            Write your own, or import files friends send you. You can also drop
            them anywhere on this page.
          </p>
          <div className="library-empty__actions">
            <Link to="/new" className="button lg">
              Create your first recipe
            </Link>
            <button
              type="button"
              className="secondary lg"
              onClick={picker.open}
            >
              Import a .crisp file
            </button>
          </div>
        </section>
      ) : (
        <>
          <DraftList
            drafts={drafts}
            now={now}
            onDiscard={(draft) => setPending({ kind: "discard", draft })}
          />

          {(loading || entries.length > 0) && (
            <div className="library__toolbar">
              <label className="library__search">
                <span className="visually-hidden">Search recipes</span>
                <input
                  type="search"
                  placeholder="Search recipes"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  disabled={loading}
                  enterKeyHint="search"
                />
              </label>
              <label className="library__sort">
                <span className="visually-hidden">Sort by</span>
                <select
                  value={sort}
                  onChange={(event) => changeSort(event.target.value)}
                  disabled={loading}
                >
                  {SORT_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          )}

          {loading ? (
            <div className="recipe-grid" aria-busy="true">
              {Array.from({ length: SKELETONS }, (_, i) => (
                <RecipeCardSkeleton key={i} />
              ))}
            </div>
          ) : visible.length > 0 ? (
            <section className="recipe-grid" aria-label="Recipes">
              {visible.map((entry) => (
                <RecipeCard
                  key={entry.record.docId}
                  entry={entry}
                  now={now}
                  onExport={(e) => void exportOne(e)}
                  onDelete={(e) => setPending({ kind: "delete", entry: e })}
                />
              ))}
            </section>
          ) : entries.length > 0 ? (
            <p className="library__none muted">
              No recipes match “{query.trim()}”.{" "}
              <button
                type="button"
                className="ghost"
                onClick={() => setQuery("")}
              >
                Clear search
              </button>
            </p>
          ) : null}

          {entries && entries.length > 0 && (
            <footer className="library__footer">
              <button
                type="button"
                className="ghost"
                onClick={() => void exportAll()}
                disabled={exporting !== null}
              >
                {exporting ? (
                  <>
                    <Spinner /> {exporting}
                  </>
                ) : (
                  "Export all recipes"
                )}
              </button>
            </footer>
          )}
        </>
      )}

      <ConfirmDialog
        open={pending !== null}
        title={
          pending?.kind === "delete"
            ? `Delete “${entryTitle(pending.entry)}”?`
            : "Discard this draft?"
        }
        confirmLabel={pending?.kind === "delete" ? "Delete" : "Discard"}
        destructive
        onConfirm={() => void confirmPending()}
        onCancel={() => setPending(null)}
      >
        <p>
          {pending?.kind === "delete"
            ? "The file is removed from this device. You can undo right after."
            : "Your unsaved changes will be lost."}
        </p>
      </ConfirmDialog>

      {undo && (
        <UndoToast
          message={`Deleted “${undo.record.title || "recipe"}”`}
          token={undo.token}
          onUndo={() => void undoRemove()}
          onClose={() => setUndo(null)}
        />
      )}
    </div>
  );
}
