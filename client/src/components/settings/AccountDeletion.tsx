import { useState } from "react";
import { errorMessage } from "../../lib/crisp";
import { useIdentity } from "../../lib/identity";
import { createLogger } from "../../lib/log";
import { deleteAccountFailureMessage } from "../../lib/serverAccount";
import { useServer } from "../../lib/servers";
import { Spinner } from "../library/ui";
import { UnlockForm } from "./PassphraseFields";

const log = createLogger("settings.account");

/**
 * Deleting the signed-in account on the server. Confirming signs in again
 * with the key file, because the server accepts only a recent sign-in, so
 * the identity must be unlocked first. The local library stays.
 */
export function AccountDeletion({ username }: { username: string }) {
  const server = useServer();
  const identity = useIdentity();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await server.deleteAccount();
    } catch (cause: unknown) {
      log.warn("delete.failed", { error: errorMessage(cause) });
      setError(deleteAccountFailureMessage(cause));
      setBusy(false);
    }
  };

  if (!open) {
    return (
      <div className="spread server__row">
        <p className="muted">
          Delete this account and everything it published on this server.
        </p>
        <button type="button" className="ghost" onClick={() => setOpen(true)}>
          Delete account
        </button>
      </div>
    );
  }
  return (
    <div
      className="banner warn confirm"
      role="alertdialog"
      aria-label="Delete account"
      data-testid="account-deletion"
    >
      <p>
        Delete <strong>{username}</strong> from {server.url}? Every recipe it
        published there is removed and the username becomes free. Your library
        on this device, and copies others already saved, stay.
      </p>
      {identity.status === "unlocked" ? (
        <div className="cluster">
          <button
            type="button"
            className="danger"
            onClick={() => void confirm()}
            disabled={busy}
          >
            {busy && <Spinner label="Deleting account" />}
            Sign in again and delete
          </button>
          <button
            type="button"
            className="secondary"
            onClick={() => setOpen(false)}
            disabled={busy}
          >
            Cancel
          </button>
        </div>
      ) : (
        <UnlockForm prompt="Unlock your identity: deleting the account signs in again with your key file." />
      )}
      {error && (
        <p className="field-error" role="alert">
          Couldn’t delete the account ({error}). It is still there.
        </p>
      )}
    </div>
  );
}
