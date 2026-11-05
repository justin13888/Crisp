import { useState } from "react";
import { LITERAL_VALUES, Trans, useTranslation } from "../../i18n";
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
  const { t } = useTranslation("settings");
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
          <Trans
            t={t}
            i18nKey="server.signedInAs"
            {...LITERAL_VALUES}
            values={{ username }}
            components={{ strong: <strong /> }}
          />
        </p>
        <button
          type="button"
          className="secondary"
          onClick={() => void server.signOut()}
        >
          {t("server.signOut")}
        </button>
      </div>
      <div className="spread server__row">
        <p className="muted">{t("server.signOutEverywhereHint")}</p>
        <button
          type="button"
          className="secondary"
          disabled={busy}
          onClick={() => void signOutEverywhere()}
        >
          {busy && <Spinner label={t("server.signingOutEverywhere")} />}
          {t("server.signOutEverywhere")}
        </button>
      </div>
      {error && (
        <p className="field-error" role="alert">
          {t("server.signOutEverywhereFailed", { error })}
        </p>
      )}
    </div>
  );
}
