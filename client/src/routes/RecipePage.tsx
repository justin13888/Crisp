import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import {
  EquipmentList,
  IngredientSections,
} from "../components/recipe/Ingredients";
import { cookResume } from "../components/recipe/model";
import { Paragraphs } from "../components/recipe/RichText";
import { ScaleControls } from "../components/recipe/ScaleControls";
import { BundleImage, StepList } from "../components/recipe/Steps";
import { TrustBadge } from "../components/recipe/TrustBadge";
import {
  type LoadedRecipe,
  useCookSession,
  useRecipe,
  useRecipeView,
} from "../components/recipe/useRecipe";
import { errorMessage, type RecipeView } from "../lib/crisp";
import { exportFile, fileName, removeEntry } from "../lib/library";
import { createLogger } from "../lib/log";
import "./recipe.css";

const log = createLogger("recipe");

/** The friendly page for a recipe that is not in the library. */
export function RecipeNotFound() {
  return (
    <section className="empty-state">
      <h1>Recipe not found</h1>
      <p className="muted">
        It may have been deleted, or the link points to a recipe that was never
        imported on this device.
      </p>
      <Link className="button" to="/">
        Back to your library
      </Link>
    </section>
  );
}

/** A placeholder while the recipe opens. */
export function RecipeLoading() {
  return (
    <section className="recipe-loading" aria-busy="true">
      <p className="muted">Opening recipe…</p>
    </section>
  );
}

/** A recipe that could not be opened. */
export function RecipeError({ message }: { message: string }) {
  return (
    <section className="empty-state">
      <h1>This recipe could not be opened</h1>
      <p className="banner bad">{message}</p>
      <Link className="button" to="/">
        Back to your library
      </Link>
    </section>
  );
}

/** `/recipe/:docId` — reading and planning a recipe. */
export function RecipePage() {
  const { docId } = useParams();
  const load = useRecipe(docId);
  switch (load.status) {
    case "loading":
      return <RecipeLoading />;
    case "missing":
      return <RecipeNotFound />;
    case "error":
      return <RecipeError message={load.message} />;
    case "ready":
      return <RecipeScreen key={load.revision} loaded={load} />;
  }
}

/** Facts under the title: yield and times. */
function RecipeFacts({ view }: { view: RecipeView }) {
  if (!view.yield_text && view.times.length === 0) return null;
  return (
    <dl className="recipe-facts">
      {view.yield_text && (
        <div>
          <dt>Makes</dt>
          <dd>{view.yield_text}</dd>
        </div>
      )}
      {view.times.map((time) => (
        <div key={time.label}>
          <dt>{time.label}</dt>
          <dd className="tabular">{time.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function RecipeScreen({ loaded }: { loaded: LoadedRecipe }) {
  const { entry, revision, stepCount, saved, languages } = loaded;
  const { record, report, bundle } = entry;
  const docId = record.docId;
  const navigate = useNavigate();
  const [state, dispatch] = useCookSession(docId, revision, stepCount, saved);
  const { view, error } = useRecipeView(bundle, state, languages);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  if (!view) return <RecipeError message={error ?? "Unknown error"} />;

  const resume = cookResume(state);
  const startLabel =
    resume.kind === "resume"
      ? `Resume step ${resume.step}`
      : resume.kind === "again"
        ? "Cook again"
        : resume.kind === "mise"
          ? "Continue cooking"
          : "Start cooking";

  const onExport = async (): Promise<void> => {
    setBusy("export");
    setActionError(null);
    try {
      const result = await exportFile(record.bytes, fileName(view.title));
      log.info("recipe.export", { docId, result });
    } catch (e: unknown) {
      log.error("recipe.export.failed", { docId, error: String(e) });
      setActionError(errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  const onDelete = async (): Promise<void> => {
    setBusy("delete");
    try {
      const removed = await removeEntry(docId);
      log.info("recipe.delete", { docId });
      navigate("/", { state: { removed } });
    } catch (e: unknown) {
      log.error("recipe.delete.failed", { docId, error: String(e) });
      setActionError(errorMessage(e));
      setBusy(null);
    }
  };

  const hasSidebarExtras = view.equipment.length > 0;

  return (
    <article className="recipe">
      <header className="recipe-hero">
        {view.media[0] && (
          <BundleImage
            bundle={bundle}
            id={view.media[0]}
            alt=""
            className="recipe-hero__image"
          />
        )}
        <div className="recipe-hero__text stack-sm">
          <h1>{view.title}</h1>
          {view.authors.length > 0 && (
            <p className="recipe-authors muted">By {view.authors.join(", ")}</p>
          )}
          <div className="recipe-summary">
            <Paragraphs paragraphs={view.summary} />
          </div>
          <RecipeFacts view={view} />
          <TrustBadge report={report} />
        </div>
      </header>

      <div className="recipe-actions">
        <Link
          className="button lg recipe-actions__start"
          to={`/cook/${encodeURIComponent(docId)}`}
          state={resume.kind === "again" ? { restart: true } : undefined}
        >
          {startLabel}
        </Link>
        <div className="cluster">
          <Link
            className="button secondary"
            to={`/edit/${encodeURIComponent(docId)}`}
          >
            Edit
          </Link>
          <button
            type="button"
            className="secondary"
            onClick={() => void onExport()}
            disabled={busy !== null}
          >
            Export
          </button>
          <button
            type="button"
            className="ghost recipe-actions__delete"
            onClick={() => setConfirmDelete(true)}
            disabled={busy !== null}
          >
            Delete
          </button>
        </div>
        {confirmDelete && (
          <div
            className="banner warn confirm"
            role="alertdialog"
            aria-label="Delete recipe"
          >
            <span>Delete “{view.title}” from this device?</span>
            <div className="cluster">
              <button
                type="button"
                className="danger"
                onClick={() => void onDelete()}
                disabled={busy !== null}
              >
                Delete
              </button>
              <button
                type="button"
                className="secondary"
                onClick={() => setConfirmDelete(false)}
              >
                Cancel
              </button>
            </div>
          </div>
        )}
        {actionError && (
          <p className="banner bad" role="alert">
            {actionError}
          </p>
        )}
      </div>

      <div className="recipe-body">
        <section className="recipe-ingredients" aria-labelledby="ingredients-h">
          <h2 id="ingredients-h">Ingredients</h2>
          <ScaleControls
            scale={state.scale}
            unitSystem={state.unitSystem}
            yieldText={view.yield_text}
            yieldAmount={view.yield_amount}
            yieldUnit={view.yield_unit}
            language={view.language}
            factorForYield={(target) => bundle.factorForYield(0, target)}
            onScale={(scale) => dispatch({ type: "setScale", scale })}
            onUnits={(unitSystem) => dispatch({ type: "setUnits", unitSystem })}
          />
          <IngredientSections sections={view.ingredients} />
          {hasSidebarExtras && (
            <>
              <h2>Equipment</h2>
              <EquipmentList equipment={view.equipment} />
            </>
          )}
        </section>

        <section className="recipe-method" aria-labelledby="method-h">
          <h2 id="method-h">Method</h2>
          <StepList
            steps={view.steps}
            docId={docId}
            title={view.title}
            bundle={bundle}
          />
        </section>
      </div>

      <RecipeFooter view={view} />
    </article>
  );
}

/** Notes, source, tags, allergens, nutrition, and license. */
function RecipeFooter({ view }: { view: RecipeView }) {
  const source = view.source;
  return (
    <footer className="recipe-footer stack">
      {view.notes.length > 0 && (
        <section>
          <h2>Notes</h2>
          <Paragraphs paragraphs={view.notes} />
        </section>
      )}
      {view.nutrition.length > 0 && (
        <section>
          <h2>Nutrition</h2>
          {view.nutrition_basis && (
            <p className="muted">Per {view.nutrition_basis}</p>
          )}
          <dl className="nutrition">
            {view.nutrition.map((item) => (
              <div key={item.label}>
                <dt>{item.label}</dt>
                <dd className="tabular">{item.value}</dd>
              </div>
            ))}
          </dl>
        </section>
      )}
      {(view.tags.length > 0 || view.allergens.length > 0) && (
        <section className="stack-sm">
          {view.allergens.length > 0 && (
            <p className="cluster">
              <strong>Contains:</strong>
              {view.allergens.map((a) => (
                <span key={a} className="badge warn">
                  {a}
                </span>
              ))}
            </p>
          )}
          {view.tags.length > 0 && (
            <ul className="chips clean" aria-label="Tags">
              {view.tags.map((tag) => (
                <li key={tag} className="chip">
                  {tag}
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
      {(source || view.attribution.length > 0) && (
        <section className="recipe-source muted">
          {source && (
            <p>
              Source:{" "}
              {source.url ? (
                <a href={source.url} target="_blank" rel="noopener noreferrer">
                  {source.title ?? source.url}
                </a>
              ) : (
                source.title
              )}
              {source.author && <> by {source.author}</>}
            </p>
          )}
          <Paragraphs paragraphs={view.attribution} />
        </section>
      )}
      <p className="recipe-license muted">
        {view.license && <>License {view.license} · </>}
        Updated{" "}
        <time dateTime={new Date(view.modified_at * 1000).toISOString()}>
          {new Date(view.modified_at * 1000).toLocaleDateString()}
        </time>
      </p>
    </footer>
  );
}
