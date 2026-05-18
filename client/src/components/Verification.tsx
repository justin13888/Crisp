import type { ComplianceReport, Trust } from "../lib/types";

/** The badge tone and label for each trust level. */
const TRUST: Record<Trust, { tone: string; label: string }> = {
  unsigned: { tone: "bad", label: "Unsigned" },
  "author-signed": { tone: "warn", label: "Author-signed" },
  endorsed: { tone: "ok", label: "Endorsed" },
};

/** A compact trust badge for a verified bundle. */
export function TrustBadge({ trust }: { trust: Trust }) {
  const { tone, label } = TRUST[trust];
  return <span className={`badge ${tone}`}>{label}</span>;
}

/**
 * The full compliance report: the overall verdict plus every check that
 * failed, each with its stable `E.*` code so a problem is actionable.
 */
export function ComplianceReportView({ report }: { report: ComplianceReport }) {
  const failures = report.checks.filter((check) => check.status === "fail");
  const passed = report.checks.filter(
    (check) => check.status === "pass",
  ).length;
  const verdictTone = report.compliant ? "ok" : "bad";

  return (
    <div className="card">
      <div className="row" style={{ justifyContent: "space-between" }}>
        <strong>Verification</strong>
        <span className={`badge ${verdictTone}`}>
          {report.compliant ? "Compliant" : "Not compliant"}
        </span>
      </div>
      <p className="muted">
        Class: {report.class} · <TrustBadge trust={report.trust} /> · {passed}{" "}
        checks passed
        {failures.length > 0 ? `, ${failures.length} failed` : ""}
      </p>
      {failures.length > 0 && (
        <table className="checks">
          <tbody>
            {failures.map((check) => (
              <tr key={check.id}>
                <td>
                  <strong>{check.id}</strong>
                </td>
                <td>
                  <code>{check.code ?? "—"}</code>
                </td>
                <td>{check.detail ?? "failed"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
