import { type ChangeEvent, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ComplianceReportView } from "../components/Verification";
import { parseBundle, verifyBundle } from "../lib/crisp";
import { saveBundle } from "../lib/db";
import { resolveText } from "../lib/render";
import type { BundlePayload, ComplianceReport } from "../lib/types";

/** A bundle the user has picked but not yet committed to the library. */
interface Pending {
  bytes: Uint8Array;
  payload: BundlePayload;
  report: ComplianceReport;
  title: string;
}

/** The import flow: pick a `.crisp` file, verify it, then save it locally. */
export function Import() {
  const navigate = useNavigate();
  const [pending, setPending] = useState<Pending | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const onFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    setBusy(true);
    setError(null);
    setPending(null);
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const report = await verifyBundle(bytes);
      const payload = await parseBundle(bytes);
      const title = resolveText(
        payload.meta.title,
        [],
        payload.default_language,
      );
      setPending({ bytes, payload, report, title });
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const save = async () => {
    if (!pending) return;
    await saveBundle({
      docId: pending.payload.doc_id,
      kind: pending.payload.kind,
      title: pending.title,
      bytes: pending.bytes,
      report: pending.report,
      published: pending.report.trust === "endorsed",
      importedAt: Date.now(),
    });
    navigate(`/recipe/${encodeURIComponent(pending.payload.doc_id)}`);
  };

  return (
    <section>
      <h1>Import a bundle</h1>
      <p className="muted">
        Choose a <code>.crisp</code> file. It is verified locally before
        anything is saved.
      </p>
      <label>
        <span>Bundle file</span>
        <input
          type="file"
          accept=".crisp,application/crisp+cbor"
          onChange={onFile}
          disabled={busy}
        />
      </label>

      {busy && <p className="muted">Verifying…</p>}
      {error && (
        <div className="banner bad">Could not read this file: {error}</div>
      )}

      {pending && (
        <>
          <h2>{pending.title || "Untitled bundle"}</h2>
          <ComplianceReportView report={pending.report} />
          {!pending.report.compliant && (
            <div className="banner warn">
              This bundle is not compliant. You can still save it, but treat its
              contents with caution.
            </div>
          )}
          <button type="button" onClick={save}>
            Save to library
          </button>
        </>
      )}
    </section>
  );
}
