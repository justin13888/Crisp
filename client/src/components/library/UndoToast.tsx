import { useEffect } from "react";
import { useTranslation } from "../../i18n";

/** How long the undo offer stays up. */
export const UNDO_MS = 10_000;

/** Props of {@link UndoToast}. */
export interface UndoToastProps {
  /** What happened, e.g. "Deleted “Soup”". */
  message: string;
  /** Changes when a new offer replaces the old one (restarts the timer). */
  token: number;
  /** Reverts the action. */
  onUndo(): void;
  /** Hides the toast (timeout or close). */
  onClose(): void;
}

/** A floating "Deleted — Undo" bar that hides itself after a while. */
export function UndoToast({ message, token, onUndo, onClose }: UndoToastProps) {
  const { t } = useTranslation("library");
  // biome-ignore lint/correctness/useExhaustiveDependencies: restart per offer
  useEffect(() => {
    const timer = setTimeout(onClose, UNDO_MS);
    return () => clearTimeout(timer);
  }, [token]);

  return (
    <div className="toast undo-toast" role="status" aria-live="polite">
      <span>{message}</span>
      <span className="cluster">
        <button type="button" onClick={onUndo}>
          {t("undo.undo")}
        </button>
        <button
          type="button"
          className="undo-toast__close"
          aria-label={t("undo.dismiss")}
          onClick={onClose}
        >
          ×
        </button>
      </span>
    </div>
  );
}
