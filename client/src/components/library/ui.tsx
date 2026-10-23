/**
 * Small UI building blocks shared by the library, import, browse, and
 * settings screens: a spinner, a trust badge, and a confirmation dialog.
 * Styles live in `routes/library.css`.
 */

import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import type { Trust } from "../../lib/crisp";
import { trustDisplay } from "./libraryModel";

/** An inline busy indicator. Decorative unless given a label. */
export function Spinner({ label }: { label?: string }) {
  return label ? (
    <span className="spinner" role="status" aria-label={label} />
  ) : (
    <span className="spinner" aria-hidden="true" />
  );
}

/** A verification trust badge with an explanatory tooltip. */
export function TrustBadge({ trust }: { trust: Trust }) {
  const display = trustDisplay(trust);
  const tone = display.tone === "neutral" ? "" : ` ${display.tone}`;
  return (
    <span
      className={`badge trust-badge${tone}`}
      title={display.description}
      data-trust={trust}
    >
      {display.label}
    </span>
  );
}

/** Props of {@link ConfirmDialog}. */
export interface ConfirmDialogProps {
  /** Whether the dialog is shown. */
  open: boolean;
  /** The question. */
  title: string;
  /** Consequences, in a sentence or two. */
  children?: ReactNode;
  /** The confirming action's label. */
  confirmLabel: string;
  /** Styles the confirming action as destructive. */
  destructive?: boolean;
  /** When set, the cook must type this word to confirm. */
  requireText?: string;
  /** Called on confirmation. */
  onConfirm(): void;
  /** Called on cancel, Escape, or backdrop click. */
  onCancel(): void;
}

/** A modal confirmation built on the native `<dialog>`. */
export function ConfirmDialog({
  open,
  title,
  children,
  confirmLabel,
  destructive = false,
  requireText,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const [typed, setTyped] = useState("");

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) {
      setTyped("");
      if (typeof dialog.showModal === "function") dialog.showModal();
      else dialog.setAttribute("open", "");
    } else if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);

  const allowed =
    !requireText ||
    typed.trim().toLowerCase() === requireText.trim().toLowerCase();

  return (
    <dialog
      ref={ref}
      className="confirm-dialog"
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onCancel();
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape") onCancel();
      }}
    >
      {open && (
        <form
          method="dialog"
          className="stack"
          onSubmit={(event) => {
            event.preventDefault();
            if (allowed) onConfirm();
          }}
        >
          <h2 id={titleId}>{title}</h2>
          {children}
          {requireText && (
            <label>
              <span>
                Type <strong>{requireText}</strong> to confirm
              </span>
              <input
                value={typed}
                onChange={(event) => setTyped(event.target.value)}
                autoComplete="off"
                autoCapitalize="off"
                spellCheck={false}
              />
            </label>
          )}
          <div className="cluster confirm-dialog__actions">
            <button type="button" className="secondary" onClick={onCancel}>
              Cancel
            </button>
            <button
              type="submit"
              className={destructive ? "danger" : undefined}
              disabled={!allowed}
            >
              {confirmLabel}
            </button>
          </div>
        </form>
      )}
    </dialog>
  );
}
