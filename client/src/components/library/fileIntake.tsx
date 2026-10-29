/**
 * Getting `.crisp` files into the app by hand: a file picker (one hidden
 * input any button can open) and drag-and-drop anywhere on the page.
 */

import {
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "../../i18n";
import { CRISP_EXTENSION, CRISP_MEDIA_TYPE, readFile } from "../../lib/library";
import { createLogger } from "../../lib/log";
import type { ImportSource } from "./importFiles";

const log = createLogger("intake");

/** What the picker accepts. */
export const CRISP_ACCEPT = `${CRISP_EXTENSION},${CRISP_MEDIA_TYPE}`;

/** Reads picked or dropped files into import sources. */
export async function readSources(
  files: Iterable<File>,
): Promise<ImportSource[]> {
  const sources: ImportSource[] = [];
  for (const file of files) {
    try {
      sources.push({ name: file.name, bytes: await readFile(file) });
    } catch (error: unknown) {
      log.warn("read.failed", { name: file.name, error: String(error) });
    }
  }
  return sources;
}

/** A hidden multi-file input plus a function that opens it. */
export function useFilePicker(onFiles: (files: File[]) => void): {
  open(): void;
  input: ReactNode;
} {
  const { t } = useTranslation("library");
  const ref = useRef<HTMLInputElement>(null);
  const open = useCallback(() => ref.current?.click(), []);
  const input = (
    <span className="visually-hidden">
      <input
        ref={ref}
        type="file"
        multiple
        accept={CRISP_ACCEPT}
        tabIndex={-1}
        aria-label={t("intake.choose")}
        data-testid="crisp-file-input"
        onChange={(event) => {
          const files = Array.from(event.currentTarget.files ?? []);
          event.currentTarget.value = "";
          if (files.length > 0) onFiles(files);
        }}
      />
    </span>
  );
  return { open, input };
}

/** Whether a drag carries files (not text or links). */
function carriesFiles(event: DragEvent): boolean {
  return Array.from(event.dataTransfer?.types ?? []).includes("Files");
}

/**
 * Accepts files dropped anywhere on the window while mounted. Returns
 * whether files are being dragged over the page (to show a drop target).
 */
export function useWindowFileDrop(onFiles: (files: File[]) => void): boolean {
  const [dragging, setDragging] = useState(false);
  const handler = useRef(onFiles);
  handler.current = onFiles;

  useEffect(() => {
    let depth = 0;
    const enter = (event: DragEvent) => {
      if (!carriesFiles(event)) return;
      event.preventDefault();
      depth++;
      setDragging(true);
    };
    const over = (event: DragEvent) => {
      if (!carriesFiles(event)) return;
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    };
    const leave = (event: DragEvent) => {
      if (!carriesFiles(event)) return;
      depth = Math.max(0, depth - 1);
      if (depth === 0) setDragging(false);
    };
    const drop = (event: DragEvent) => {
      if (!carriesFiles(event)) return;
      event.preventDefault();
      depth = 0;
      setDragging(false);
      const files = Array.from(event.dataTransfer?.files ?? []);
      log.debug("drop", { count: files.length });
      if (files.length > 0) handler.current(files);
    };
    window.addEventListener("dragenter", enter);
    window.addEventListener("dragover", over);
    window.addEventListener("dragleave", leave);
    window.addEventListener("drop", drop);
    return () => {
      window.removeEventListener("dragenter", enter);
      window.removeEventListener("dragover", over);
      window.removeEventListener("drop", drop);
      window.removeEventListener("dragleave", leave);
    };
  }, []);

  return dragging;
}

/** The full-page drop target shown while files are dragged over. */
export function DropOverlay({ visible }: { visible: boolean }) {
  const { t } = useTranslation("library");
  if (!visible) return null;
  return (
    <div className="drop-overlay" aria-hidden="true">
      <div className="drop-overlay__panel">
        <span className="drop-overlay__icon">⇣</span>
        {t("intake.drop")}
      </div>
    </div>
  );
}
