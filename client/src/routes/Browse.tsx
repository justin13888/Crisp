import "./library.css";
import { useCallback, useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import {
  type BrowseFilters,
  EMPTY_FILTERS,
  type FacetKind,
  facetCount,
  facetLabel,
  hasFilters,
  mayHaveMore,
  PAGE_SIZE,
  pageRequest,
  toggleFacet,
} from "../components/library/browseModel";
import { Spinner } from "../components/library/ui";
import { useEngine } from "../hooks/useEngine";
import { Trans, useTranslation } from "../i18n";
import {
  type CrispServer,
  errorMessage,
  type RemoteRecipe,
} from "../lib/crisp";
import { addFile } from "../lib/library";
import { createLogger } from "../lib/log";
import { useServer } from "../lib/servers";
import { getStore } from "../lib/store";

const log = createLogger("browse");

/** A saved-state per result. */
type SaveState =
  | { kind: "saving" }
  | { kind: "saved" }
  | { kind: "failed"; reason: string };

/** One group of facet chips. */
function FacetGroup({
  title,
  kind,
  values,
  filters,
  onToggle,
}: {
  title: string;
  kind: FacetKind;
  values: string[];
  filters: BrowseFilters;
  onToggle(kind: FacetKind, value: string): void;
}) {
  if (values.length === 0) return null;
  return (
    <fieldset className="facet-group">
      <legend>{title}</legend>
      <div className="chips">
        {values.map((value) => (
          <button
            key={value}
            type="button"
            className="chip"
            aria-pressed={filters[kind].includes(value)}
            onClick={() => onToggle(kind, value)}
          >
            {facetLabel(value)}
          </button>
        ))}
      </div>
    </fieldset>
  );
}

/** One published recipe with its save action. */
function ResultCard({
  recipe,
  inLibrary,
  save,
  onSave,
}: {
  recipe: RemoteRecipe;
  inLibrary: boolean;
  save: SaveState | undefined;
  onSave(): void;
}) {
  const { t } = useTranslation("browse");
  const open = `/recipe/${encodeURIComponent(recipe.doc_id)}`;
  return (
    <article className="card result-card">
      <h3 className="result-card__title">{recipe.title}</h3>
      {recipe.authors.length > 0 && (
        <p className="muted result-card__authors">
          {t("result.by", { authors: recipe.authors.join(", ") })}
        </p>
      )}
      {recipe.summary && (
        <p className="result-card__summary">{recipe.summary}</p>
      )}
      {recipe.tags.length > 0 && (
        <ul
          className="chips clean result-card__tags"
          aria-label={t("result.tags")}
        >
          {recipe.tags.map((tag) => (
            <li key={tag} className="chip">
              {tag}
            </li>
          ))}
        </ul>
      )}
      <div className="cluster result-card__actions">
        {inLibrary || save?.kind === "saved" ? (
          <>
            <span className="badge ok">{t("result.inLibrary")}</span>
            <Link className="button secondary" to={open}>
              {t("result.open")}
            </Link>
          </>
        ) : (
          <button
            type="button"
            onClick={onSave}
            disabled={save?.kind === "saving"}
          >
            {save?.kind === "saving" && <Spinner label={t("result.saving")} />}
            {t("result.save")}
          </button>
        )}
      </div>
      {save?.kind === "failed" && (
        <p className="field-error" role="alert">
          {save.reason}
        </p>
      )}
    </article>
  );
}

/** Published recipes on the active server. */
export function Browse() {
  const { t } = useTranslation("browse");
  const server = useServer();
  const { engine } = useEngine();
  const [text, setText] = useState("");
  const [filters, setFilters] = useState<BrowseFilters>(EMPTY_FILTERS);
  const [results, setResults] = useState<RemoteRecipe[]>([]);
  const [loading, setLoading] = useState(false);
  const [more, setMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [local, setLocal] = useState<Set<string>>(new Set());
  const [saves, setSaves] = useState<Record<string, SaveState>>({});
  const request = useRef(0);
  const online = server.status === "online";
  const { client } = server;

  useEffect(() => {
    void getStore()
      .then((store) => store.bundles.all())
      .then((records) => setLocal(new Set(records.map((r) => r.docId))))
      .catch((cause: unknown) =>
        log.warn("local.failed", { error: errorMessage(cause) }),
      );
  }, []);

  const fetchPage = useCallback(
    async (wanted: BrowseFilters, offset: number) => {
      const id = ++request.current;
      setLoading(true);
      setError(null);
      const started = performance.now();
      const req = pageRequest(wanted, offset);
      let api: CrispServer | null = null;
      try {
        api = await client();
        const page =
          req.kind === "browse"
            ? await api.browse(req.limit, req.offset)
            : await api.search(req.query);
        if (id !== request.current) return;
        setResults((previous) =>
          offset === 0 ? page.recipes : [...previous, ...page.recipes],
        );
        setMore(mayHaveMore(page.recipes.length, PAGE_SIZE));
        log.debug("page", {
          kind: req.kind,
          offset,
          count: page.recipes.length,
          ms: Math.round(performance.now() - started),
        });
      } catch (cause: unknown) {
        if (id !== request.current) return;
        log.warn("page.failed", { error: errorMessage(cause) });
        setError(errorMessage(cause));
      } finally {
        api?.free();
        if (id === request.current) setLoading(false);
      }
    },
    [client],
  );

  useEffect(() => {
    if (online) void fetchPage(filters, 0);
  }, [online, filters, fetchPage]);

  const save = async (recipe: RemoteRecipe) => {
    setSaves((s) => ({ ...s, [recipe.doc_id]: { kind: "saving" } }));
    let api: CrispServer | null = null;
    try {
      api = await client();
      const bytes = await api.download(recipe.doc_id, recipe.revision);
      if (!bytes) throw new Error(t("result.gone"));
      await addFile(bytes);
      setSaves((s) => ({ ...s, [recipe.doc_id]: { kind: "saved" } }));
      setLocal((set) => new Set(set).add(recipe.doc_id));
      log.info("saved", { docId: recipe.doc_id, revision: recipe.revision });
    } catch (cause: unknown) {
      log.warn("save.failed", {
        docId: recipe.doc_id,
        error: errorMessage(cause),
      });
      setSaves((s) => ({
        ...s,
        [recipe.doc_id]: { kind: "failed", reason: errorMessage(cause) },
      }));
    } finally {
      api?.free();
    }
  };

  if (!online) {
    return (
      <div className="browse">
        <h1>{t("title")}</h1>
        {server.status === "connecting" || server.status === "idle" ? (
          <p className="muted">
            <Spinner /> {t("offline.connecting", { url: server.url })}
          </p>
        ) : (
          <section className="card browse-offline" data-testid="browse-offline">
            <h2>{t("offline.title")}</h2>
            <p>
              <Trans
                t={t}
                i18nKey="offline.body"
                values={{ url: server.url }}
                components={{ strong: <strong /> }}
              />
            </p>
            <p className="muted">{t("offline.local")}</p>
            <div className="cluster">
              <button type="button" onClick={() => void server.refresh()}>
                {t("offline.retry")}
              </button>
              <Link to="/settings" className="button secondary">
                {t("offline.settings")}
              </Link>
            </div>
          </section>
        )}
      </div>
    );
  }

  const constraints = engine?.recipeConstraints();
  const count = facetCount(filters);

  return (
    <div className="browse">
      <h1>{t("title")}</h1>
      <form
        className="browse__search"
        onSubmit={(event) => {
          event.preventDefault();
          setFilters((f) => ({ ...f, q: text }));
        }}
      >
        <label>
          <span className="visually-hidden">{t("search.label")}</span>
          <input
            type="search"
            value={text}
            placeholder={t("search.label")}
            onChange={(event) => setText(event.target.value)}
            enterKeyHint="search"
          />
        </label>
        <button type="submit">{t("search.submit")}</button>
      </form>

      {constraints && (
        <details className="facets" open={count > 0}>
          <summary>
            {count > 0
              ? t("filters.titleCount", { number: count })
              : t("filters.title")}
          </summary>
          <FacetGroup
            title={t("filters.course")}
            kind="course"
            values={constraints.course}
            filters={filters}
            onToggle={(kind, value) =>
              setFilters((f) => toggleFacet(f, kind, value))
            }
          />
          <FacetGroup
            title={t("filters.diet")}
            kind="diet"
            values={constraints.diet}
            filters={filters}
            onToggle={(kind, value) =>
              setFilters((f) => toggleFacet(f, kind, value))
            }
          />
          <FacetGroup
            title={t("filters.allergenFree")}
            kind="allergen_free"
            values={constraints.allergens}
            filters={filters}
            onToggle={(kind, value) =>
              setFilters((f) => toggleFacet(f, kind, value))
            }
          />
          {hasFilters(filters) && (
            <button
              type="button"
              className="ghost"
              onClick={() => {
                setText("");
                setFilters(EMPTY_FILTERS);
              }}
            >
              {t("filters.clear")}
            </button>
          )}
        </details>
      )}

      {error && (
        <p className="banner bad" role="alert">
          {t("loadFailed", { error })}
        </p>
      )}

      <div className="results" aria-busy={loading}>
        {results.map((recipe) => (
          <ResultCard
            key={`${recipe.doc_id}@${recipe.revision}`}
            recipe={recipe}
            inLibrary={local.has(recipe.doc_id)}
            save={saves[recipe.doc_id]}
            onSave={() => void save(recipe)}
          />
        ))}
      </div>

      {!loading && !error && results.length === 0 && (
        <p className="muted">
          {hasFilters(filters) ? t("empty.filtered") : t("empty.none")}
        </p>
      )}

      {loading ? (
        <p className="muted">
          <Spinner /> {t("loading")}
        </p>
      ) : (
        more && (
          <button
            type="button"
            className="secondary lg load-more"
            onClick={() => void fetchPage(filters, results.length)}
          >
            {t("loadMore")}
          </button>
        )
      )}
    </div>
  );
}
