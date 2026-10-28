import type { RefObject } from "react";
import { useTranslation } from "../../i18n";
import type { FieldProblem } from "./model";

/** The list of problems at the top; each jumps to its field. */
export function ProblemSummary({
  problems,
  expanded,
  summaryRef,
  onPick,
}: {
  problems: FieldProblem[];
  expanded: boolean;
  summaryRef: RefObject<HTMLElement | null>;
  onPick: (field: string) => void;
}) {
  const { t } = useTranslation("editor");
  const list = (
    <ul className="clean problem-list">
      {problems.map((p, i) => (
        <li key={`${p.field}-${i}`}>
          {p.field ? (
            <button
              type="button"
              className="ghost problem-link"
              onClick={() => p.field && onPick(p.field)}
            >
              <strong>{t("problems.field", { label: p.label })}</strong>{" "}
              {p.message}
            </button>
          ) : (
            <span>
              <strong>{t("problems.field", { label: p.label })}</strong>{" "}
              {p.message}
            </span>
          )}
        </li>
      ))}
    </ul>
  );
  const count = t("problems.count", { count: problems.length });
  return (
    <section
      ref={summaryRef}
      id="editor-problems"
      className="problem-summary"
      tabIndex={-1}
      aria-label={t("problems.label")}
    >
      {problems.length === 0 ? null : expanded ? (
        <div className="banner bad">
          <p>
            <strong>{count}</strong>
          </p>
          {list}
        </div>
      ) : (
        <details className="problem-details">
          <summary className="muted">{count}</summary>
          {list}
        </details>
      )}
    </section>
  );
}
