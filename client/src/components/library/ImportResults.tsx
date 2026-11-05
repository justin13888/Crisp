import { Link } from "react-router-dom";
import { useTranslation } from "../../i18n";
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

/** The catalogue key of each outcome's status badge. */
const STATUS_LABEL = {
  added: "results.status.added",
  updated: "results.status.updated",
  failed: "results.status.failed",
} as const;

/** One file's result: status, title, trust, and why it failed. */
function ResultRow({ row, detailed }: { row: ImportRow; detailed: boolean }) {
  const { t } = useTranslation("library");
  const { outcome } = row;
  if (!outcome) {
    return (
      <li className="import-row" data-status="pending">
        <Spinner />
        <span className="import-row__name">{row.name}</span>
        <span className="muted">{t("results.verifying")}</span>
      </li>
    );
  }
  if (outcome.status === "failed") {
    return (
      <li className="import-row" data-status="failed">
        <span className="badge bad">{t(STATUS_LABEL.failed)}</span>
        <div className="import-row__body">
          <span className="import-row__name">
            {outcome.title
              ? `${outcome.title} · ${outcome.name}`
              : outcome.name}
          </span>
          <span className="import-row__reason">{outcome.reason}</span>
          {detailed && outcome.failedChecks.length > 0 && (
            <table className="checks">
              <caption className="visually-hidden">
                {t("results.failedChecks")}
              </caption>
              <tbody>
                {outcome.failedChecks.map((check) => (
                  <tr key={`${check.id}-${check.code ?? ""}`}>
                    <td>
                      <code>{check.code ?? check.id}</code>
                    </td>
                    <td>{check.detail ?? t(STATUS_LABEL.failed)}</td>
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
      <span className="badge ok">{t(STATUS_LABEL[outcome.status])}</span>
      <div className="import-row__body">
        <span className="import-row__name">
          {outcome.title || outcome.name}
        </span>
        <span className="cluster import-row__meta">
          <TrustBadge trust={outcome.trust} />
          {detailed && <span className="muted">{t("results.allPassed")}</span>}
        </span>
      </div>
      <Link
        className="button secondary import-row__open"
        to={`/recipe/${encodeURIComponent(outcome.docId)}`}
      >
        {t("results.open")}
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
  const { t } = useTranslation("library");
  if (rows.length === 0) return null;
  const done = rows.flatMap((row) => (row.outcome ? [row.outcome] : []));
  const pending = rows.length - done.length;
  const summary =
    pending > 0
      ? t("results.importing", { count: rows.length })
      : describeTally(tallyOutcomes(done));
  return (
    <section className="card import-results" aria-label={t("results.label")}>
      <div className="spread">
        <h2 className="import-results__title" aria-live="polite">
          {summary}
        </h2>
        {onDismiss && pending === 0 && (
          <button
            type="button"
            className="ghost icon"
            aria-label={t("results.dismiss")}
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
