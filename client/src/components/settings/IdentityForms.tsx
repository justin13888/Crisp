import { useId, useState } from "react";
import { useTranslation } from "../../i18n";
import { errorMessage } from "../../lib/crisp";
import { useIdentity } from "../../lib/identity";
import { readFile } from "../../lib/library";
import { createLogger } from "../../lib/log";
import { Spinner } from "../library/ui";
import { PassphraseInput, StrengthMeter } from "./PassphraseFields";
import { passphraseProblem } from "./settingsModel";

const log = createLogger("settings.identity");

/** Creates a new identity: display name and a confirmed passphrase. */
export function CreateIdentityForm() {
  const { t } = useTranslation("settings");
  const identity = useIdentity();
  const [name, setName] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const meterId = useId();

  const problem = !name.trim()
    ? t("create.nameRequired")
    : passphraseProblem(passphrase, confirm);

  const submit = async () => {
    setTouched(true);
    if (problem) return;
    setBusy(true);
    setError(null);
    try {
      await identity.create(name, passphrase);
      log.info("create.done");
    } catch (cause: unknown) {
      log.error("create.failed", { error: errorMessage(cause) });
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="stack-sm"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <label>
        <span>{t("displayName.label")}</span>
        <input
          value={name}
          onChange={(event) => setName(event.target.value)}
          autoComplete="name"
          placeholder={t("create.namePlaceholder")}
          maxLength={80}
        />
      </label>
      <PassphraseInput
        label={t("passphrase.label")}
        value={passphrase}
        onChange={setPassphrase}
        autoComplete="new-password"
        describedBy={meterId}
      />
      <StrengthMeter passphrase={passphrase} id={meterId} />
      <PassphraseInput
        label={t("create.confirmPassphrase")}
        value={confirm}
        onChange={setConfirm}
        autoComplete="new-password"
      />
      {touched && problem && (
        <p className="field-error" role="alert">
          {problem}
        </p>
      )}
      {error && (
        <p className="field-error" role="alert">
          {error}
        </p>
      )}
      <button type="submit" className="lg" disabled={busy}>
        {busy ? (
          <>
            <Spinner label={t("create.creatingLabel")} /> {t("create.creating")}
          </>
        ) : (
          t("create.submit")
        )}
      </button>
    </form>
  );
}

/** Restores an identity from a `.crispkey` backup. */
export function ImportKeyForm() {
  const { t } = useTranslation("settings");
  const identity = useIdentity();
  const [file, setFile] = useState<File | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    if (!file || !name.trim()) {
      setError(t("import.missing"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await identity.importKeyFile(await readFile(file), name);
      log.info("import.done");
    } catch (cause: unknown) {
      log.warn("import.failed", { error: errorMessage(cause) });
      setError(t("import.invalid", { error: errorMessage(cause) }));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="stack-sm"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <label>
        <span>{t("import.file")}</span>
        <input
          type="file"
          accept=".crispkey,application/octet-stream"
          onChange={(event) => setFile(event.target.files?.[0] ?? null)}
        />
      </label>
      <label>
        <span>{t("displayName.label")}</span>
        <input
          value={name}
          onChange={(event) => setName(event.target.value)}
          autoComplete="name"
          maxLength={80}
        />
      </label>
      {error && (
        <p className="field-error" role="alert">
          {error}
        </p>
      )}
      <button type="submit" className="secondary" disabled={busy}>
        {busy ? <Spinner label={t("import.restoring")} /> : null}
        {t("import.submit")}
      </button>
      <p className="muted small">{t("import.hint")}</p>
    </form>
  );
}
