/**
 * The recipe editor at `/new` and `/edit/:docId`: one sectioned page over
 * an in-memory form, validated live by the engine, autosaved to the local
 * store, and sealed into a signed `.crisp` file on save.
 */

import "./editor.css";
import {
  type RefObject,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Link, useParams } from "react-router-dom";
import type { Issue } from "../../generated/crisp";
import { useEngine } from "../../hooks/useEngine";
import { type Engine, errorMessage } from "../../lib/crisp";
import { openEntry } from "../../lib/library";
import { createLogger } from "../../lib/log";
import { BasicsSection } from "./BasicsSection";
import { EditorProvider, type EditorContextValue, focusField } from "./context";
import { type DraftResult, draftToForm, formToDraft, newForm } from "./convert";
import { ClassifySection, NotesSection } from "./DetailsSection";
import { deleteDraft, draftIdFor, loadDraft, saveDraft } from "./drafts";
import { EquipmentSection } from "./EquipmentSection";
import { IngredientsSection } from "./IngredientsSection";
import { issuesToProblems, mergeProblems, problemsByField } from "./issues";
import {
  defaultTempUnit,
  type FieldProblem,
  type FormState,
  primaryLanguage,
} from "./model";
import { PhotosSection } from "./PhotosSection";
import { SavePanel } from "./SavePanel";
import { StepsSection } from "./StepsSection";

const log = createLogger("editor");

/** Idle time before the form is autosaved. */
const AUTOSAVE_MS = 500;
/** Idle time before the form is re-validated. */
const VALIDATE_MS = 300;

const SECTIONS = [
  { id: "sec-basics", label: "Basics" },
  { id: "sec-ingredients", label: "Ingredients" },
  { id: "sec-equipment", label: "Equipment" },
  { id: "sec-steps", label: "Steps" },
  { id: "sec-photos", label: "Photos" },
  { id: "sec-tags", label: "Tags" },
  { id: "sec-notes", label: "Notes" },
  { id: "sec-save", label: "Save" },
] as const;

/** Routes `/new` and `/edit/:docId` (a fresh editor per document). */
export function Editor() {
  const { docId } = useParams();
  return <EditorScreen key={docId ?? "new"} docId={docId} />;
}

/** Builds the draft and runs every check the engine would on save. */
function check(
  form: FormState,
  engine: Engine,
): DraftResult & { all: FieldProblem[] } {
  const started = performance.now();
  const result = formToDraft(form, engine);
  let engineProblems: FieldProblem[];
  try {
    engineProblems = issuesToProblems(
      engine.validateDraft(result.draft),
      result.index,
    );
  } catch (cause: unknown) {
    engineProblems = [
      { field: null, label: "Recipe", message: errorMessage(cause) },
    ];
  }
  const all = mergeProblems(result.problems, engineProblems);
  log.debug("validate", {
    problems: all.length,
    ms: Math.round(performance.now() - started),
  });
  return { ...result, all };
}

interface Loaded {
  /** The form as stored (or the empty template) — what "discard" returns to. */
  fresh: FormState;
  /** The stored file being revised. */
  previous: Uint8Array | null;
}

type AutosaveStatus = "idle" | "pending" | "saved" | "error";

function EditorScreen({ docId }: { docId: string | undefined }) {
  const { engine, error: engineError } = useEngine();
  const draftId = draftIdFor(docId);
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [form, setForm] = useState<FormState | null>(null);
  const formRef = useRef<FormState | null>(null);
  const [restored, setRestored] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [problems, setProblems] = useState<FieldProblem[]>([]);
  const [touched, setTouched] = useState<ReadonlySet<string>>(new Set());
  const [showAll, setShowAll] = useState(false);
  const [autosave, setAutosave] = useState<AutosaveStatus>("idle");
  const pendingFocus = useRef<string | null>(null);
  const pendingSave = useRef<FormState | null>(null);
  const inflight = useRef<Promise<void>>(Promise.resolve());
  const savedToLibrary = useRef(false);
  const summaryRef = useRef<HTMLElement>(null);

  // Load the stored recipe (or the template) and any autosaved draft.
  useEffect(() => {
    if (!engine) return;
    let live = true;
    (async () => {
      let fresh: FormState;
      let previous: Uint8Array | null = null;
      if (docId) {
        const entry = await openEntry(docId);
        if (!entry) throw new Error("That recipe is not in your library.");
        try {
          const payload = entry.bundle.draft();
          if (payload.kind !== "recipe") {
            throw new Error("Books cannot be edited here yet.");
          }
          fresh = draftToForm(payload, engine);
        } finally {
          entry.bundle.free();
        }
        previous = entry.record.bytes;
      } else {
        const locale = navigator.language;
        const lang = primaryLanguage(locale);
        fresh = newForm({
          lang,
          identity: engine.newIdentity("", lang, 0),
          tempUnit: defaultTempUnit(locale),
        });
      }
      const saved = await loadDraft(draftId).catch((cause: unknown) => {
        log.warn("draft.load.failed", { draftId, error: errorMessage(cause) });
        return null;
      });
      if (!live) return;
      formRef.current = saved ?? fresh;
      setLoaded({ fresh, previous });
      setForm(saved ?? fresh);
      setRestored(Boolean(saved));
      log.info("open", { draftId, restored: Boolean(saved) });
    })().catch((cause: unknown) => {
      log.warn("open.failed", { draftId, error: errorMessage(cause) });
      if (live) setLoadError(errorMessage(cause));
    });
    return () => {
      live = false;
    };
  }, [engine, docId, draftId]);

  const update = useCallback((change: (form: FormState) => FormState) => {
    const current = formRef.current;
    if (!current) return;
    const next = change(current);
    if (next === current) return;
    formRef.current = next;
    setForm(next);
    setDirty(true);
  }, []);

  const create = useCallback<EditorContextValue["create"]>(
    (change) => {
      let ref = -1;
      update((current) => {
        const result = change(current);
        ref = result.ref;
        return result.form;
      });
      return ref;
    },
    [update],
  );

  // Autosave after a short idle.
  useEffect(() => {
    if (!form || !dirty || savedToLibrary.current) return;
    pendingSave.current = form;
    setAutosave("pending");
    const timer = setTimeout(() => {
      const snapshot = pendingSave.current;
      if (!snapshot || savedToLibrary.current) return;
      pendingSave.current = null;
      inflight.current = saveDraft(draftId, snapshot).then(
        () => {
          if (!savedToLibrary.current) setAutosave("saved");
        },
        (cause: unknown) => {
          log.warn("autosave.failed", { draftId, error: errorMessage(cause) });
          setAutosave("error");
        },
      );
    }, AUTOSAVE_MS);
    return () => clearTimeout(timer);
  }, [form, dirty, draftId]);

  // Flush a pending autosave when the page is hidden or the editor closes.
  useEffect(() => {
    const flush = () => {
      const snapshot = pendingSave.current;
      if (!snapshot || savedToLibrary.current) return;
      pendingSave.current = null;
      inflight.current = saveDraft(draftId, snapshot).catch((cause: unknown) =>
        log.warn("autosave.flush.failed", { error: errorMessage(cause) }),
      );
    };
    const onHide = () => {
      if (document.visibilityState === "hidden") flush();
    };
    document.addEventListener("visibilitychange", onHide);
    window.addEventListener("pagehide", flush);
    return () => {
      document.removeEventListener("visibilitychange", onHide);
      window.removeEventListener("pagehide", flush);
      flush();
    };
  }, [draftId]);

  // Validate after a short idle.
  useEffect(() => {
    if (!form || !engine) return;
    const timer = setTimeout(
      () => setProblems(check(form, engine).all),
      VALIDATE_MS,
    );
    return () => clearTimeout(timer);
  }, [form, engine]);

  // Focus a field requested by an action, once it has rendered.
  useEffect(() => {
    const id = pendingFocus.current;
    if (id && form) {
      pendingFocus.current = null;
      requestAnimationFrame(() => document.getElementById(id)?.focus());
    }
  });

  const focusLater = useCallback((id: string) => {
    pendingFocus.current = id;
  }, []);

  const touch = useCallback((id: string) => {
    setTouched((previous) => {
      if (previous.has(id)) return previous;
      const next = new Set(previous);
      next.add(id);
      return next;
    });
  }, []);

  const showProblems = useCallback((list: FieldProblem[]) => {
    setProblems(list);
    setShowAll(true);
    requestAnimationFrame(() => {
      summaryRef.current?.scrollIntoView({
        block: "start",
        behavior: "smooth",
      });
      summaryRef.current?.focus({ preventScroll: true });
    });
  }, []);

  const prepare = useCallback(() => {
    const current = formRef.current;
    if (!current || !engine) return null;
    const result = check(current, engine);
    if (result.all.length > 0) {
      log.info("save.blocked", { problems: result.all.length });
      showProblems(result.all);
      return null;
    }
    return result.draft;
  }, [engine, showProblems]);

  const onIssues = useCallback(
    (issues: Issue[]) => {
      const current = formRef.current;
      if (!current || !engine) return;
      showProblems(
        issuesToProblems(issues, formToDraft(current, engine).index),
      );
    },
    [engine, showProblems],
  );

  const onSaved = useCallback(async () => {
    savedToLibrary.current = true;
    pendingSave.current = null;
    await inflight.current;
    await deleteDraft(draftId).catch((cause: unknown) =>
      log.warn("draft.delete.failed", { draftId, error: errorMessage(cause) }),
    );
    setDirty(false);
    setAutosave("idle");
  }, [draftId]);

  const discard = useCallback(async () => {
    if (!loaded) return;
    pendingSave.current = null;
    await inflight.current;
    await deleteDraft(draftId);
    formRef.current = loaded.fresh;
    setForm(loaded.fresh);
    setDirty(false);
    setRestored(false);
    setAutosave("idle");
    setTouched(new Set());
    setShowAll(false);
    log.info("draft.discarded", { draftId });
  }, [draftId, loaded]);

  const vocab = useMemo(() => {
    const constraints = engine?.recipeConstraints();
    return {
      course: constraints?.course ?? [],
      diet: constraints?.diet ?? [],
      allergens: constraints?.allergens ?? [],
      maxSlugBytes: constraints?.limits["maxSlugBytes"] ?? Infinity,
    };
  }, [engine]);

  const visible = useMemo(
    () =>
      problems.filter(
        (p) => showAll || (p.field !== null && touched.has(p.field)),
      ),
    [problems, showAll, touched],
  );
  const byField = useMemo(() => problemsByField(visible), [visible]);

  const context = useMemo<EditorContextValue | null>(
    () =>
      engine && form
        ? {
            engine,
            form,
            update,
            create,
            problems: byField,
            touch,
            focusLater,
            vocab,
          }
        : null,
    [engine, form, update, create, byField, touch, focusLater, vocab],
  );

  const heading = docId ? "Edit recipe" : "New recipe";
  if (engineError || loadError) {
    return (
      <div className="stack">
        <h1>{heading}</h1>
        <p className="banner bad" role="alert">
          {loadError ?? `The recipe engine failed to load: ${engineError}`}
        </p>
        <Link to="/">Back to the library</Link>
      </div>
    );
  }
  if (!context || !loaded) {
    return (
      <p className="muted" aria-busy="true">
        Loading…
      </p>
    );
  }

  return (
    <EditorProvider value={context}>
      <div className="editor">
        <nav className="editor-nav" aria-label="Recipe sections">
          <ul className="clean">
            {SECTIONS.map((s) => (
              <li key={s.id}>
                <a href={`#${s.id}`}>{s.label}</a>
              </li>
            ))}
          </ul>
        </nav>
        <div className="editor-body">
          <header className="editor-header spread">
            <h1>{heading}</h1>
            <span className="muted autosave" aria-live="polite">
              {autosave === "saved"
                ? "Draft saved"
                : autosave === "pending"
                  ? "Saving draft…"
                  : autosave === "error"
                    ? "Draft not saved"
                    : ""}
            </span>
          </header>
          {restored ? (
            <div className="banner warn spread" role="status">
              <span>Restored your unsaved draft — discard?</span>
              <span className="cluster">
                <button
                  type="button"
                  className="secondary"
                  onClick={() => void discard()}
                >
                  Discard draft
                </button>
                <button
                  type="button"
                  className="ghost"
                  onClick={() => setRestored(false)}
                >
                  Keep
                </button>
              </span>
            </div>
          ) : null}
          <ProblemSummary
            problems={problems}
            expanded={showAll}
            summaryRef={summaryRef}
            onPick={(field) => {
              touch(field);
              focusField(field);
            }}
          />
          <BasicsSection />
          <IngredientsSection />
          <EquipmentSection />
          <StepsSection />
          <PhotosSection />
          <ClassifySection />
          <NotesSection />
          <SavePanel
            prepare={prepare}
            previous={loaded.previous}
            onIssues={onIssues}
            onSaved={onSaved}
          />
        </div>
      </div>
    </EditorProvider>
  );
}

/** The list of problems at the top; each jumps to its field. */
function ProblemSummary({
  problems,
  expanded,
  summaryRef,
  onPick,
}: {
  problems: FieldProblem[];
  expanded: boolean;
  summaryRef: RefObject<HTMLElement | null>;
  onPick: (field: string) => void;
}) {
  const list = (
    <ul className="clean problem-list">
      {problems.map((p, i) => (
        <li key={`${p.field}-${i}`}>
          {p.field ? (
            <button
              type="button"
              className="ghost problem-link"
              onClick={() => p.field && onPick(p.field)}
            >
              <strong>{p.label}:</strong> {p.message}
            </button>
          ) : (
            <span>
              <strong>{p.label}:</strong> {p.message}
            </span>
          )}
        </li>
      ))}
    </ul>
  );
  const count = `${problems.length} ${problems.length === 1 ? "thing" : "things"} to fix before saving`;
  return (
    <section
      ref={summaryRef}
      id="editor-problems"
      className="problem-summary"
      tabIndex={-1}
      aria-label="Problems"
    >
      {problems.length === 0 ? null : expanded ? (
        <div className="banner bad">
          <p>
            <strong>{count}</strong>
          </p>
          {list}
        </div>
      ) : (
        <details className="problem-details">
          <summary className="muted">{count}</summary>
          {list}
        </details>
      )}
    </section>
  );
}
