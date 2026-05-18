import type { ReactNode } from "react";
import type { Prose as ProseModel, Run } from "../lib/types";

/** Renders one styled run. Links are rendered only when `https` (spec D7). */
function RunSpan({ run }: { run: Run }) {
  let node: ReactNode = run.t;
  const style = run.style ?? [];
  if (style.includes("code")) node = <code>{node}</code>;
  if (style.includes("underline")) node = <u>{node}</u>;
  if (style.includes("italic")) node = <em>{node}</em>;
  if (style.includes("bold")) node = <strong>{node}</strong>;
  if (run.link?.startsWith("https://")) {
    node = (
      <a href={run.link} target="_blank" rel="noreferrer">
        {node}
      </a>
    );
  }
  return node;
}

/**
 * Renders pre-parsed Crisp prose as React nodes.
 *
 * Prose is structured data, never a markup string, so there is no HTML
 * injection surface — runs become plain elements and links are scheme-checked.
 */
export function Prose({ prose }: { prose: ProseModel }) {
  return (
    <>
      {prose.map((paragraph, pi) => (
        <p key={pi}>
          {paragraph.map((run, ri) => (
            <RunSpan key={ri} run={run} />
          ))}
        </p>
      ))}
    </>
  );
}
