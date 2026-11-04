/**
 * Removing a published document from the server, from its recipe page:
 * this revision, or every revision the signed-in account published of it.
 * The copy on this device always stays.
 */

import { useState } from "react";
import { Spinner } from "../components/library/ui";
import { errorMessage } from "../lib/crisp";
import { markUnpublished, publishedHere } from "../lib/library";
import { createLogger } from "../lib/log";
import { unpublishFailureMessage } from "../lib/serverAccount";
import { useServer } from "../lib/servers";

const log = createLogger("recipe.unpublish");

/** What to remove: one revision, or every revision of the document. */
type Target = "revision" | "document";

/**
 * The unpublish controls for a document this device published to the
 * active server as the signed-in account; nothing for any other document.
 */
export function UnpublishActions({
  docId,
  revision,
  publishedTo,
}: {
  docId: string;
  /** The revision held here; `""` when unknown, which offers only the document. */
  revision: string;
  /** The record's stored `publishedTo`, narrowed here. */
  publishedTo: unknown;
}) {
  const server = useServer();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState<Target | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [removed, setRemoved] = useState<Target | null>(null);

  const username = server.username;
  if (!username) return null;
  if (removed) {
    return (
      <p className="banner" role="status">
        {removed === "revision"
          ? "This revision is no longer on the server."
          : "The server no longer offers your revisions of this recipe."}{" "}
        Your copy on this device stays.
      </p>
    );
  }
  if (!publishedHere(publishedTo, server.url, username)) return null;

  const remove = async (target: Target) => {
    setBusy(target);
    setError(null);
    try {
      await server.unpublish(docId, target === "revision" ? revision : null);
      if (target === "document") {
        await markUnpublished(docId, server.url, username);
      }
      log.info("unpublished", { docId, target });
      setRemoved(target);
    } catch (cause: unknown) {
      log.warn("unpublish.failed", {
        docId,
        target,
        error: errorMessage(cause),
      });
      setError(unpublishFailureMessage(cause));
    } finally {
      setBusy(null);
    }
  };

  if (!open) {
    return (
      <button
        type="button"
        className="ghost"
        onClick={() => setOpen(true)}
        disabled={busy !== null}
      >
        Remove from server
      </button>
    );
  }
  return (
    <div
      className="banner warn confirm"
      role="alertdialog"
      aria-label="Remove from server"
      data-testid="unpublish-actions"
    >
      <span>
        Remove this recipe from {server.url}? Copies others already saved, and
        yours on this device, stay.
      </span>
      <div className="cluster">
        {revision && (
          <button
            type="button"
            className="danger"
            onClick={() => void remove("revision")}
            disabled={busy !== null}
          >
            {busy === "revision" && <Spinner label="Removing" />}
            This revision
          </button>
        )}
        <button
          type="button"
          className="danger"
          onClick={() => void remove("document")}
          disabled={busy !== null}
        >
          {busy === "document" && <Spinner label="Removing" />}
          All my revisions
        </button>
        <button
          type="button"
          className="secondary"
          onClick={() => setOpen(false)}
          disabled={busy !== null}
        >
          Cancel
        </button>
      </div>
      {error && (
        <p className="field-error" role="alert">
          Couldn’t remove it: {error}
        </p>
      )}
    </div>
  );
}
