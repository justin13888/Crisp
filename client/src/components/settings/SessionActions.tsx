import { useState } from "react";
import { errorMessage } from "../../lib/crisp";
import { createLogger } from "../../lib/log";
import { useServer } from "../../lib/servers";
import { Spinner } from "../library/ui";

const log = createLogger("settings.session");

/**
 * The signed-in session's controls: sign out on this device, or sign out
 * everywhere, which ends every session of the account on the server.
 */
export function SessionActions({ username }: { username: string }) {
  const server = useServer();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const signOutEverywhere = async () => {
    setBusy(true);
    setError(null);
    try {
      await server.signOutEverywhere();
    } catch (cause: unknown) {
      log.warn("signout-everywhere.failed", { error: errorMessage(cause) });
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div data-testid="session-actions">
      <div className="spread server__row">
        <p>
          Signed in as <strong>{username}</strong>
        </p>
        <button
          type="button"
          className="secondary"
          onClick={() => void server.signOut()}
        >
          Sign out
        </button>
      </div>
      <div className="spread server__row">
        <p className="muted">
          Lost a device? End every session of this account, here and on your
          other devices.
        </p>
        <button
          type="button"
          className="secondary"
          disabled={busy}
          onClick={() => void signOutEverywhere()}
        >
          {busy && <Spinner label="Signing out everywhere" />}
          Sign out everywhere
        </button>
      </div>
      {error && (
        <p className="field-error" role="alert">
          Couldn’t sign out everywhere ({error}). You are still signed in here;
          try again.
        </p>
      )}
    </div>
  );
}
