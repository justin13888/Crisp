import { type FormEvent, useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { fromHex, type RecipeSummary } from "../lib/api";
import { parseBundle, verifyBundle } from "../lib/crisp";
import { saveBundle } from "../lib/db";
import { useIdentity } from "../lib/identity";
import { resolveText } from "../lib/render";
import type { PublicKeySet } from "../lib/types";

/** Browse and search recipes on the connected server, and import them. */
export function Browse() {
  const { server } = useIdentity();
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<RecipeSummary[] | null>(null);
  const [platformKeys, setPlatformKeys] = useState<PublicKeySet[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    server
      .serverInfo()
      .then((info) =>
        setPlatformKeys(
          info.platform_keys.map((key) => ({
            ed25519: fromHex(key.ed25519),
            "ml-dsa-65": fromHex(key.ml_dsa_65),
          })),
        ),
      )
      .catch(() => setPlatformKeys([]));
  }, [server]);

  const load = useCallback(async (action: () => Promise<RecipeSummary[]>) => {
    setError(null);
    try {
      setResults(await action());
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setResults([]);
    }
  }, []);

  useEffect(() => {
    void load(() => server.browse());
  }, [server, load]);

  const onSearch = (event: FormEvent) => {
    event.preventDefault();
    void load(() =>
      query.trim() ? server.search({ q: query.trim() }) : server.browse(),
    );
  };

  const openRemote = async (summary: RecipeSummary) => {
    setBusy(summary.doc_id);
    setError(null);
    try {
      const bytes = await server.fetchBundle(summary.doc_id);
      if (!bytes) throw new Error("the server no longer has this bundle");
      const report = await verifyBundle(bytes, platformKeys);
      const payload = await parseBundle(bytes);
      await saveBundle({
        docId: payload.doc_id,
        kind: payload.kind,
        title: resolveText(payload.meta.title, [], payload.default_language),
        bytes,
        report,
        published: true,
        importedAt: Date.now(),
      });
      navigate(`/recipe/${encodeURIComponent(payload.doc_id)}`);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section>
      <h1>Browse the server</h1>
      <form className="row" onSubmit={onSearch}>
        <input
          style={{ flex: 1 }}
          placeholder="Search recipes…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button type="submit">Search</button>
      </form>

      {error && <div className="banner bad">{error}</div>}
      {!results && <p className="muted">Loading…</p>}
      {results && results.length === 0 && (
        <p className="muted">No recipes found.</p>
      )}

      {results?.map((summary) => (
        <div
          className="card"
          key={`${summary.doc_id}/${summary.recipe_revision}`}
        >
          <strong>{summary.title}</strong>
          {summary.summary && <p className="muted">{summary.summary}</p>}
          {summary.cuisine.length + summary.diet.length > 0 && (
            <div className="chips">
              {[...summary.cuisine, ...summary.diet].map((tag) => (
                <span className="chip" key={tag}>
                  {tag}
                </span>
              ))}
            </div>
          )}
          <button
            type="button"
            onClick={() => openRemote(summary)}
            disabled={busy === summary.doc_id}
          >
            {busy === summary.doc_id ? "Importing…" : "Open & verify"}
          </button>
        </div>
      ))}
    </section>
  );
}
