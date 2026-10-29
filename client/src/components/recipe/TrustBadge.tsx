import { useTranslation } from "../../i18n";
import type { ComplianceReport } from "../../lib/crisp";
import { Icon } from "./Icon";
import { failedChecks, trustInfo } from "./model";

/**
 * The trust badge with a collapsible verification panel listing every
 * failed check (and a count of the passing ones).
 */
export function TrustBadge({ report }: { report: ComplianceReport }) {
  const { t } = useTranslation("recipe");
  const info = trustInfo(report.trust);
  const failed = failedChecks(report.checks);
  const passed = report.checks.filter((c) => c.status === "pass").length;
  const skipped = report.checks.filter((c) => c.status === "skipped").length;
  return (
    <details className="trust">
      <summary>
        <span className={`badge ${info.tone}`}>
          <Icon name="shield" /> {info.label}
        </span>
        <span className="trust__more muted">{t("trust.details")}</span>
      </summary>
      <div className="trust__panel">
        <p>{info.description}</p>
        <p className="muted">
          {t("trust.passed", { count: passed })}
          {skipped > 0 ? t("trust.skipped", { count: skipped }) : ""}
          {failed.length > 0 ? t("trust.failed", { count: failed.length }) : ""}
          {t("trust.end")}
        </p>
        {failed.length > 0 && (
          <table className="checks">
            <tbody>
              {failed.map((check) => (
                <tr key={check.id}>
                  <td>
                    <span className="badge bad">{check.id}</span>
                  </td>
                  <td>
                    {check.code && <code>{check.code}</code>}{" "}
                    {check.detail ?? t("trust.checkFailed")}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </details>
  );
}
