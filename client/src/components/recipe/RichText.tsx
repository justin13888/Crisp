import type { ReactNode } from "react";
import type { RunView } from "../../lib/crisp";

/** A paragraph of styled runs, as rendered by the engine. */
export type Paragraph = readonly RunView[];

/** One styled run; links (already verified `https`) open in a new tab. */
function Run({ run }: { run: RunView }) {
  let node: ReactNode = run.text;
  if (run.code) node = <code>{node}</code>;
  if (run.underline) node = <u>{node}</u>;
  if (run.italic) node = <em>{node}</em>;
  if (run.bold) node = <strong>{node}</strong>;
  if (run.link) {
    node = (
      <a href={run.link} target="_blank" rel="noopener noreferrer">
        {node}
      </a>
    );
  }
  return <>{node}</>;
}

/** Inline runs without a wrapping paragraph. */
export function Runs({ runs }: { runs: Paragraph }) {
  return (
    <>
      {runs.map((run, i) => (
        <Run key={i} run={run} />
      ))}
    </>
  );
}

/** Rich-text paragraphs. */
export function Paragraphs({
  paragraphs,
  className,
}: {
  paragraphs: readonly Paragraph[];
  className?: string;
}) {
  if (paragraphs.length === 0) return null;
  return (
    <>
      {paragraphs.map((runs, i) => (
        <p key={i} className={className}>
          <Runs runs={runs} />
        </p>
      ))}
    </>
  );
}

/** The plain text of paragraphs (for announcements and titles). */
export function plainText(paragraphs: readonly Paragraph[]): string {
  return paragraphs
    .map((runs) => runs.map((run) => run.text).join(""))
    .join(" ");
}
