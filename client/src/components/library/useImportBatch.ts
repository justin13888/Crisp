import { useCallback, useRef, useState } from "react";
import type { ImportRow } from "./ImportResults";
import {
  type ImportDeps,
  type ImportOutcome,
  type ImportSource,
  importAll,
} from "./importFiles";
import { libraryImportDeps } from "./libraryDeps";

/** An import batch's live rows and how to start one. */
export interface ImportBatch {
  /** Every file of the current batches, in arrival order. */
  rows: ImportRow[];
  /** Whether files are still being verified. */
  busy: boolean;
  /** Imports files, appending their rows; resolves with the outcomes. */
  run(sources: ImportSource[]): Promise<ImportOutcome[]>;
  /** Forgets the shown results. */
  clear(): void;
}

/** Runs imports one file at a time, updating each row as it finishes. */
export function useImportBatch(
  deps: ImportDeps = libraryImportDeps,
): ImportBatch {
  const [rows, setRows] = useState<ImportRow[]>([]);
  const [running, setRunning] = useState(0);
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const total = useRef(0);

  const run = useCallback(
    (sources: ImportSource[]) => {
      if (sources.length === 0) return Promise.resolve([]);
      const base = total.current;
      total.current += sources.length;
      setRows((previous) => [
        ...previous,
        ...sources.map((s) => ({ name: s.name, outcome: null })),
      ]);
      setRunning((n) => n + 1);
      // Batches run one after another so rows fill in order.
      const job = queue.current.then(() =>
        importAll(sources, deps, (outcome, index) => {
          setRows((previous) =>
            previous.map((row, i) =>
              i === base + index ? { ...row, outcome } : row,
            ),
          );
        }).finally(() => setRunning((n) => n - 1)),
      );
      queue.current = job.catch(() => undefined);
      return job;
    },
    [deps],
  );

  const clear = useCallback(() => {
    total.current = 0;
    setRows([]);
  }, []);

  return { rows, busy: running > 0, run, clear };
}
