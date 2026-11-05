import { useId, useState } from "react";
import { useTranslation } from "../../i18n";
import { errorMessage } from "../../lib/crisp";
import { useIdentity } from "../../lib/identity";
import { createLogger } from "../../lib/log";
import { Spinner } from "../library/ui";
import { passphraseStrength } from "./settingsModel";

const log = createLogger("settings.identity");

/** A passphrase input with a show/hide toggle. */
export function PassphraseInput({
  label,
  value,
  onChange,
  autoComplete,
  autoFocus = false,
  describedBy,
}: {
  label: string;
  value: string;
  onChange(value: string): void;
  autoComplete: "new-password" | "current-password";
  autoFocus?: boolean;
  describedBy?: string;
}) {
  const { t } = useTranslation("settings");
  const [shown, setShown] = useState(false);
  const id = useId();
  return (
    <div className="passphrase-field">
      <label htmlFor={id}>{label}</label>
      <div className="passphrase-input">
        <input
          id={id}
          type={shown ? "text" : "password"}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          autoComplete={autoComplete}
          autoCapitalize="off"
          spellCheck={false}
          // biome-ignore lint/a11y/noAutofocus: inline unlock asks for input
          autoFocus={autoFocus}
          aria-describedby={describedBy}
        />
        <button
          type="button"
          className="ghost passphrase-input__toggle"
          aria-label={t("passphrase.show")}
          aria-pressed={shown}
          onClick={() => setShown((v) => !v)}
        >
          {shown ? t("passphrase.hideButton") : t("passphrase.showButton")}
        </button>
      </div>
    </div>
  );
}

/** A strength meter with a hint for a new passphrase. */
export function StrengthMeter({
  passphrase,
  id,
}: {
  passphrase: string;
  id: string;
}) {
  const strength = passphraseStrength(passphrase);
  return (
    <div className="strength" id={id} data-score={strength.score}>
      <div className="strength__bar" aria-hidden="true">
        {[1, 2, 3, 4].map((step) => (
          <span key={step} className={step <= strength.score ? "on" : ""} />
        ))}
      </div>
      <p className="strength__text">
        {strength.label && <strong>{strength.label}. </strong>}
        {strength.hint}
      </p>
    </div>
  );
}

/** Unlocks the stored identity with its passphrase. */
export function UnlockForm({
  autoFocus = false,
  prompt,
}: {
  autoFocus?: boolean;
  prompt?: string;
}) {
  const { t } = useTranslation("settings");
  const identity = useIdentity();
  const [passphrase, setPassphrase] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const errorId = useId();

  const submit = async () => {
    if (!passphrase) return;
    setBusy(true);
    setError(null);
    try {
      await identity.unlock(passphrase);
      setPassphrase("");
    } catch (cause: unknown) {
      log.info("unlock.failed", { error: errorMessage(cause) });
      setError(t("unlock.failed"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="unlock-form"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      {prompt && <p className="muted">{prompt}</p>}
      <PassphraseInput
        label={t("passphrase.label")}
        value={passphrase}
        onChange={setPassphrase}
        autoComplete="current-password"
        autoFocus={autoFocus}
        describedBy={error ? errorId : undefined}
      />
      {error && (
        <p className="field-error" id={errorId} role="alert">
          {error}
        </p>
      )}
      <button type="submit" disabled={busy || !passphrase}>
        {busy ? (
          <>
            <Spinner label={t("unlock.unlockingLabel")} />{" "}
            {t("unlock.unlocking")}
          </>
        ) : (
          t("unlock.submit")
        )}
      </button>
    </form>
  );
}
