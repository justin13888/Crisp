import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { RecipeView } from "../components/RecipeView";
import { ComplianceReportView } from "../components/Verification";
import { parseBundle } from "../lib/crisp";
import { getBundle, type StoredBundle } from "../lib/db";
import type { BundlePayload } from "../lib/types";

/** The detail page for one stored bundle: its recipe(s) and its verdict. */
export function RecipePage() {
  const { docId } = useParams<{ docId: string }>();
  const [bundle, setBundle] = useState<StoredBundle | null>(null);
  const [payload, setPayload] = useState<BundlePayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [missing, setMissing] = useState(false);

  useEffect(() => {
    if (!docId) return;
    let cancelled = false;
    (async () => {
      const stored = await getBundle(docId);
      if (cancelled) return;
      if (!stored) {
        setMissing(true);
        return;
      }
      setBundle(stored);
      try {
        setPayload(await parseBundle(stored.bytes));
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e));
      }
    })().catch((e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
    return () => {
      cancelled = true;
    };
  }, [docId]);

  if (missing) {
    return (
      <div className="card">
        <p>This bundle is not in your library.</p>
        <Link to="/">Back to the library</Link>
      </div>
    );
  }
  if (error)
    return (
      <div className="banner bad">Could not open this bundle: {error}</div>
    );
  if (!bundle || !payload) return <p className="muted">Loading…</p>;

  const downloadUrl = URL.createObjectURL(
    new Blob([bundle.bytes as BlobPart], { type: "application/crisp+cbor" }),
  );

  return (
    <section>
      <ComplianceReportView report={bundle.report} />
      <p>
        <a
          href={downloadUrl}
          download={`${bundle.docId}.crisp`}
          className="button secondary"
        >
          Export .crisp file
        </a>
      </p>
      {payload.recipes.map((recipe) => (
        <RecipeView key={recipe.id} recipe={recipe} payload={payload} />
      ))}
    </section>
  );
}
