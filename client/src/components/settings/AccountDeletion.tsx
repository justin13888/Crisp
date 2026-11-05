import { useState } from "react";
import { LITERAL_VALUES, Trans, useTranslation } from "../../i18n";
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
  const { t } = useTranslation("settings");
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
        <p className="muted">{t("account.deleteHint")}</p>
        <button type="button" className="ghost" onClick={() => setOpen(true)}>
          {t("account.delete")}
        </button>
      </div>
    );
  }
  return (
    <div
      className="banner warn confirm"
      role="alertdialog"
      aria-label={t("account.delete")}
      data-testid="account-deletion"
    >
      <p>
        <Trans
          t={t}
          i18nKey="account.deleteQuestion"
          {...LITERAL_VALUES}
          values={{ username, url: server.url }}
          components={{ strong: <strong /> }}
        />
      </p>
      {identity.status === "unlocked" ? (
        <div className="cluster">
          <button
            type="button"
            className="danger"
            onClick={() => void confirm()}
            disabled={busy}
          >
            {busy && <Spinner label={t("account.deleting")} />}
            {t("account.confirm")}
          </button>
          <button
            type="button"
            className="secondary"
            onClick={() => setOpen(false)}
            disabled={busy}
          >
            {t("account.cancel")}
          </button>
        </div>
      ) : (
        <UnlockForm prompt={t("account.unlockPrompt")} />
      )}
      {error && (
        <p className="field-error" role="alert">
          {t("account.failed", { error })}
        </p>
      )}
    </div>
  );
}
