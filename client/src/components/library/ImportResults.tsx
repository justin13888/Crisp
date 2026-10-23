import { Link } from "react-router-dom";
import {
  describeTally,
  type ImportOutcome,
  tallyOutcomes,
} from "./importFiles";
import { Spinner, TrustBadge } from "./ui";

/** One file in an import batch: still verifying, or done. */
export interface ImportRow {
  /** The file name. */
  name: string;
  /** The result, once known. */
  outcome: ImportOutcome | null;
}

const STATUS_LABEL = {
  added: "Added",
  updated: "Updated",
  failed: "Failed",
} as const;

/** One file's result: status, title, trust, and why it failed. */
function ResultRow({ row, detailed }: { row: ImportRow; detailed: boolean }) {
  const { outcome } = row;
  if (!outcome) {
    return (
      <li className="import-row" data-status="pending">
        <Spinner />
        <span className="import-row__name">{row.name}</span>
        <span className="muted">Verifying…</span>
      </li>
    );
  }
  if (outcome.status === "failed") {
    return (
      <li className="import-row" data-status="failed">
        <span className="badge bad">{STATUS_LABEL.failed}</span>
        <div className="import-row__body">
          <span className="import-row__name">
            {outcome.title
              ? `${outcome.title} · ${outcome.name}`
              : outcome.name}
          </span>
          <span className="import-row__reason">{outcome.reason}</span>
          {detailed && outcome.failedChecks.length > 0 && (
            <table className="checks">
              <caption className="visually-hidden">Failed checks</caption>
              <tbody>
                {outcome.failedChecks.map((check) => (
                  <tr key={`${check.id}-${check.code ?? ""}`}>
                    <td>
                      <code>{check.code ?? check.id}</code>
                    </td>
                    <td>{check.detail ?? "Failed"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </li>
    );
  }
  return (
    <li className="import-row" data-status={outcome.status}>
      <span className="badge ok">{STATUS_LABEL[outcome.status]}</span>
      <div className="import-row__body">
        <span className="import-row__name">
          {outcome.title || outcome.name}
        </span>
        <span className="cluster import-row__meta">
          <TrustBadge trust={outcome.trust} />
          {detailed && <span className="muted">All checks passed</span>}
        </span>
      </div>
      <Link
        className="button secondary import-row__open"
        to={`/recipe/${encodeURIComponent(outcome.docId)}`}
      >
        Open
      </Link>
    </li>
  );
}

/** Props of {@link ImportResults}. */
export interface ImportResultsProps {
  /** The batch, in order. */
  rows: ImportRow[];
  /** Show failed checks in full (the import screen). */
  detailed?: boolean;
  /** Called when the cook dismisses the panel. */
  onDismiss?: () => void;
}

/** The per-file results of an import batch. */
export function ImportResults({
  rows,
  detailed = false,
  onDismiss,
}: ImportResultsProps) {
  if (rows.length === 0) return null;
  const done = rows.flatMap((row) => (row.outcome ? [row.outcome] : []));
  const pending = rows.length - done.length;
  const summary =
    pending > 0
      ? `Importing ${rows.length} file${rows.length === 1 ? "" : "s"}…`
      : describeTally(tallyOutcomes(done));
  return (
    <section className="card import-results" aria-label="Import results">
      <div className="spread">
        <h2 className="import-results__title" aria-live="polite">
          {summary}
        </h2>
        {onDismiss && pending === 0 && (
          <button
            type="button"
            className="ghost icon"
            aria-label="Dismiss import results"
            onClick={onDismiss}
          >
            ×
          </button>
        )}
      </div>
      <ul className="clean import-results__list">
        {rows.map((row, index) => (
          <ResultRow
            key={`${index}-${row.name}`}
            row={row}
            detailed={detailed}
          />
        ))}
      </ul>
    </section>
  );
}
