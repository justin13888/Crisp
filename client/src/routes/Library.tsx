import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { TrustBadge } from "../components/Verification";
import { allBundles, deleteBundle, type StoredBundle } from "../lib/db";

/** The local library: every `.crisp` bundle held offline in this browser. */
export function Library() {
  const [bundles, setBundles] = useState<StoredBundle[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = () => {
    allBundles()
      .then(setBundles)
      .catch((e: unknown) =>
        setError(e instanceof Error ? e.message : String(e)),
      );
  };

  useEffect(reload, []);

  const remove = async (docId: string) => {
    await deleteBundle(docId);
    reload();
  };

  if (error)
    return (
      <div className="banner bad">
        Could not open the local library: {error}
      </div>
    );
  if (!bundles) return <p className="muted">Loading library…</p>;

  return (
    <section>
      <div className="row" style={{ justifyContent: "space-between" }}>
        <h1>Your library</h1>
        <div className="row">
          <Link className="button secondary" to="/import">
            Import a file
          </Link>
          <Link className="button" to="/new">
            New recipe
          </Link>
        </div>
      </div>

      {bundles.length === 0 && (
        <div className="card">
          <p>Your library is empty.</p>
          <p className="muted">
            Import a <code>.crisp</code> file, create a recipe, or browse a
            server to get started.
          </p>
        </div>
      )}

      {bundles.map((bundle) => (
        <div className="card" key={bundle.docId}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <Link
              className="card-link"
              to={`/recipe/${encodeURIComponent(bundle.docId)}`}
            >
              <strong>{bundle.title || "Untitled"}</strong>
            </Link>
            <TrustBadge trust={bundle.report.trust} />
          </div>
          <p className="muted">
            {bundle.kind} · {bundle.published ? "published" : "local"} ·{" "}
            <code>{bundle.docId.slice(0, 20)}…</code>
          </p>
          <button
            type="button"
            className="secondary"
            onClick={() => remove(bundle.docId)}
          >
            Remove
          </button>
        </div>
      ))}
    </section>
  );
}
