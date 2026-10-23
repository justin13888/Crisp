import { useId, useRef, useState } from "react";
import { errorMessage } from "../../lib/crisp";
import { useIdentity } from "../../lib/identity";
import { exportFile, readFile } from "../../lib/library";
import { createLogger } from "../../lib/log";
import { ConfirmDialog, Spinner } from "../library/ui";
import { PassphraseInput, StrengthMeter, UnlockForm } from "./PassphraseFields";
import { passphraseProblem, truncateId } from "./settingsModel";

const log = createLogger("settings.identity");

/** The backup file name. */
export const KEY_FILE_NAME = "crisp-identity.crispkey";

/** Creates a new identity: display name and a confirmed passphrase. */
function CreateIdentityForm() {
  const identity = useIdentity();
  const [name, setName] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const meterId = useId();

  const problem = !name.trim()
    ? "Enter the name your recipes will be signed with."
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
        <span>Display name</span>
        <input
          value={name}
          onChange={(event) => setName(event.target.value)}
          autoComplete="name"
          placeholder="e.g. Grandma Rose"
          maxLength={80}
        />
      </label>
      <PassphraseInput
        label="Passphrase"
        value={passphrase}
        onChange={setPassphrase}
        autoComplete="new-password"
        describedBy={meterId}
      />
      <StrengthMeter passphrase={passphrase} id={meterId} />
      <PassphraseInput
        label="Confirm passphrase"
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
            <Spinner label="Creating your key" /> Creating your key…
          </>
        ) : (
          "Create identity"
        )}
      </button>
    </form>
  );
}

/** Restores an identity from a `.crispkey` backup. */
function ImportKeyForm() {
  const identity = useIdentity();
  const [file, setFile] = useState<File | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    if (!file || !name.trim()) {
      setError("Choose a backup file and enter your display name.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await identity.importKeyFile(await readFile(file), name);
      log.info("import.done");
    } catch (cause: unknown) {
      log.warn("import.failed", { error: errorMessage(cause) });
      setError(`That isn’t a valid Crisp key file: ${errorMessage(cause)}`);
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
        <span>Backup file (.crispkey)</span>
        <input
          type="file"
          accept=".crispkey,application/octet-stream"
          onChange={(event) => setFile(event.target.files?.[0] ?? null)}
        />
      </label>
      <label>
        <span>Display name</span>
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
        {busy ? <Spinner label="Restoring" /> : null}
        Restore identity
      </button>
      <p className="muted small">
        You’ll unlock it with the passphrase you chose when it was created.
      </p>
    </form>
  );
}

/** Copies text, briefly confirming. */
function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  return (
    <button
      type="button"
      className="ghost"
      aria-label={label}
      onClick={() => {
        void navigator.clipboard?.writeText(text).then(
          () => {
            setCopied(true);
            if (timer.current) clearTimeout(timer.current);
            timer.current = setTimeout(() => setCopied(false), 1500);
          },
          (error: unknown) => log.warn("copy.failed", { error: String(error) }),
        );
      }}
    >
      {copied ? "Copied" : "Copy"}
    </button>
  );
}

/** Renames the identity inline. */
function DisplayName({ name }: { name: string }) {
  const identity = useIdentity();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(name);

  if (!editing) {
    return (
      <div className="spread identity__row">
        <div>
          <div className="identity__label">Display name</div>
          <div className="identity__name">{name}</div>
        </div>
        <button
          type="button"
          className="ghost"
          onClick={() => {
            setDraft(name);
            setEditing(true);
          }}
        >
          Edit
        </button>
      </div>
    );
  }
  return (
    <form
      className="identity__rename"
      onSubmit={(event) => {
        event.preventDefault();
        if (!draft.trim()) return;
        void identity.rename(draft).then(() => setEditing(false));
      }}
    >
      <label>
        <span>Display name</span>
        <input
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          maxLength={80}
          // biome-ignore lint/a11y/noAutofocus: the cook just asked to edit
          autoFocus
        />
      </label>
      <div className="cluster">
        <button type="submit" disabled={!draft.trim()}>
          Save
        </button>
        <button
          type="button"
          className="secondary"
          onClick={() => setEditing(false)}
        >
          Cancel
        </button>
      </div>
    </form>
  );
}

/** The identity card: create/restore, or manage the existing key. */
export function IdentityCard() {
  const identity = useIdentity();
  const [mode, setMode] = useState<"create" | "restore">("create");
  const [forgetting, setForgetting] = useState(false);
  const [backupNote, setBackupNote] = useState<string | null>(null);

  const downloadBackup = async () => {
    const bytes = await identity.exportKeyFile();
    if (!bytes) return;
    const result = await exportFile(
      bytes,
      KEY_FILE_NAME,
      "application/octet-stream",
    );
    log.info("backup.exported", { result });
    if (result !== "cancelled") {
      setBackupNote("Backup saved. Keep it somewhere safe, off this device.");
    }
  };

  return (
    <section className="card settings-card" aria-labelledby="identity-title">
      <div className="spread">
        <h2 id="identity-title">Identity</h2>
        {identity.status === "locked" && (
          <span className="badge warn" data-testid="identity-status">
            Locked
          </span>
        )}
        {identity.status === "unlocked" && (
          <span className="badge ok" data-testid="identity-status">
            Unlocked
          </span>
        )}
      </div>

      {identity.status === "loading" && (
        <div className="stack" aria-busy="true">
          <div className="skeleton skeleton-line" />
          <div className="skeleton skeleton-line skeleton-line--short" />
        </div>
      )}

      {identity.status === "none" && (
        <div className="stack">
          <p>
            Your identity signs the recipes you write, so others can tell
            they’re really yours.
          </p>
          <p className="banner warn">
            Your key lives only on this device. Back it up — if you lose it you
            cannot edit your recipes as the same author.
          </p>
          <div className="segmented">
            <button
              type="button"
              aria-pressed={mode === "create"}
              className={mode === "create" ? "" : "secondary"}
              onClick={() => setMode("create")}
            >
              Create new
            </button>
            <button
              type="button"
              aria-pressed={mode === "restore"}
              className={mode === "restore" ? "" : "secondary"}
              onClick={() => setMode("restore")}
            >
              Restore backup
            </button>
          </div>
          {mode === "create" ? <CreateIdentityForm /> : <ImportKeyForm />}
        </div>
      )}

      {(identity.status === "locked" || identity.status === "unlocked") && (
        <div className="stack">
          <DisplayName name={identity.displayName ?? ""} />
          {identity.userId && (
            <div className="spread identity__row">
              <div>
                <div className="identity__label">User id</div>
                <code className="identity__id" title={identity.userId}>
                  {truncateId(identity.userId)}
                </code>
              </div>
              <CopyButton text={identity.userId} label="Copy user id" />
            </div>
          )}

          {identity.status === "locked" ? (
            <UnlockForm prompt="Unlock your key to sign recipes and sign in to servers." />
          ) : (
            <div className="spread identity__row">
              <p className="muted">
                Your key is unlocked for signing until you lock it or close the
                app.
              </p>
              <button
                type="button"
                className="secondary"
                onClick={identity.lock}
              >
                Lock
              </button>
            </div>
          )}

          <div className="identity__backup">
            <p className="muted">
              Your key lives only on this device. Back it up — if you lose it
              you cannot edit your recipes as the same author.
            </p>
            <div className="cluster">
              <button
                type="button"
                className="secondary"
                onClick={() => void downloadBackup()}
              >
                Download backup
              </button>
              <button
                type="button"
                className="ghost danger-text"
                onClick={() => setForgetting(true)}
              >
                Forget on this device…
              </button>
            </div>
            {backupNote && (
              <p className="banner ok" role="status">
                {backupNote}
              </p>
            )}
          </div>
        </div>
      )}

      <ConfirmDialog
        open={forgetting}
        title="Forget your identity?"
        confirmLabel="Forget identity"
        destructive
        requireText="forget"
        onConfirm={() => {
          setForgetting(false);
          void identity.forget();
        }}
        onCancel={() => setForgetting(false)}
      >
        <p>
          This deletes your key from this device. Without a backup you will
          never be able to edit your recipes as the same author again.
        </p>
      </ConfirmDialog>
    </section>
  );
}
