import "./library.css";
import { useEffect, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import {
  DropOverlay,
  readSources,
  useFilePicker,
  useWindowFileDrop,
} from "../components/library/fileIntake";
import { ImportResults } from "../components/library/ImportResults";
import { useImportBatch } from "../components/library/useImportBatch";
import { onLaunchedFiles, takeSharedFiles } from "../lib/incomingFiles";
import { createLogger } from "../lib/log";

const log = createLogger("import.screen");

/**
 * The landing screen for files arriving from outside (share target, "open
 * with") and for importing by hand. Every file is verified before it is
 * stored; refusals list the failed checks.
 */
export function Import() {
  const [params, setParams] = useSearchParams();
  const batch = useImportBatch();
  const run = batch.run;
  const [over, setOver] = useState(false);
  const sharedTaken = useRef(false);

  const shared = params.get("shared") === "1";
  useEffect(() => {
    if (!shared || sharedTaken.current) return;
    sharedTaken.current = true;
    void takeSharedFiles().then((files) => {
      log.info("shared.received", { count: files.length });
      setParams({}, { replace: true });
      void run(files);
    });
  }, [shared, run, setParams]);

  useEffect(() => {
    const supported = onLaunchedFiles((files) => {
      log.info("launched.received", { count: files.length });
      void run(files);
    });
    log.debug("launch.queue", { supported });
  }, [run]);

  const importFiles = async (files: File[]) => {
    await run(await readSources(files));
  };
  const picker = useFilePicker((files) => void importFiles(files));
  const dragging = useWindowFileDrop((files) => void importFiles(files));

  return (
    <div className="import-screen stack-lg">
      {picker.input}
      <DropOverlay visible={dragging} />
      <header>
        <h1>Import recipes</h1>
        <p className="muted">
          Every <code>.crisp</code> file is checked before it joins your library
          — files that were tampered with or don’t follow the format are
          refused.
        </p>
      </header>

      <button
        type="button"
        className={`drop-zone${over ? " drop-zone--over" : ""}`}
        onClick={picker.open}
        onDragEnter={() => setOver(true)}
        onDragLeave={() => setOver(false)}
        onDrop={() => setOver(false)}
      >
        <span className="drop-zone__icon" aria-hidden="true">
          ⇣
        </span>
        <span className="drop-zone__title">Choose .crisp files</span>
        <span className="drop-zone__hint">or drop them here</span>
      </button>

      <ImportResults rows={batch.rows} detailed onDismiss={batch.clear} />

      {batch.rows.length > 0 && !batch.busy && (
        <p>
          <Link to="/">Back to your library</Link>
        </p>
      )}
    </div>
  );
}
