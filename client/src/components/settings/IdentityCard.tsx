import { useRef, useState } from "react";
import { useTranslation } from "../../i18n";
import { useIdentity } from "../../lib/identity";
import { exportFile } from "../../lib/library";
import { createLogger } from "../../lib/log";
import { ConfirmDialog } from "../library/ui";
import { CreateIdentityForm, ImportKeyForm } from "./IdentityForms";
import { UnlockForm } from "./PassphraseFields";
import { truncateId } from "./settingsModel";

const log = createLogger("settings.identity");

/** The backup file name. */
export const KEY_FILE_NAME = "crisp-identity.crispkey";

/** Copies text, briefly confirming. */
function CopyButton({ text, label }: { text: string; label: string }) {
  const { t } = useTranslation("settings");
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
      {copied ? t("copy.copied") : t("copy.copy")}
    </button>
  );
}

/** Renames the identity inline. */
function DisplayName({ name }: { name: string }) {
  const { t } = useTranslation("settings");
  const identity = useIdentity();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(name);

  if (!editing) {
    return (
      <div className="spread identity__row">
        <div>
          <div className="identity__label">{t("displayName.label")}</div>
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
          {t("displayName.edit")}
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
        <span>{t("displayName.label")}</span>
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
          {t("displayName.save")}
        </button>
        <button
          type="button"
          className="secondary"
          onClick={() => setEditing(false)}
        >
          {t("displayName.cancel")}
        </button>
      </div>
    </form>
  );
}

/** The identity card: create/restore, or manage the existing key. */
export function IdentityCard() {
  const { t } = useTranslation("settings");
  const identity = useIdentity();
  const [mode, setMode] = useState<"create" | "restore">("create");
  const [forgetting, setForgetting] = useState(false);
  const [backupSaved, setBackupSaved] = useState(false);

  const downloadBackup = async () => {
    const bytes = await identity.exportKeyFile();
    if (!bytes) return;
    const result = await exportFile(
      bytes,
      KEY_FILE_NAME,
      "application/octet-stream",
    );
    log.info("backup.exported", { result });
    if (result !== "cancelled") setBackupSaved(true);
  };

  return (
    <section className="card settings-card" aria-labelledby="identity-title">
      <div className="spread">
        <h2 id="identity-title">{t("identity.title")}</h2>
        {identity.status === "locked" && (
          <span className="badge warn" data-testid="identity-status">
            {t("identity.locked")}
          </span>
        )}
        {identity.status === "unlocked" && (
          <span className="badge ok" data-testid="identity-status">
            {t("identity.unlocked")}
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
          <p>{t("identity.intro")}</p>
          <p className="banner warn">{t("identity.backupWarning")}</p>
          <div className="segmented">
            <button
              type="button"
              aria-pressed={mode === "create"}
              className={mode === "create" ? "" : "secondary"}
              onClick={() => setMode("create")}
            >
              {t("identity.createNew")}
            </button>
            <button
              type="button"
              aria-pressed={mode === "restore"}
              className={mode === "restore" ? "" : "secondary"}
              onClick={() => setMode("restore")}
            >
              {t("identity.restoreBackup")}
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
                <div className="identity__label">{t("identity.userId")}</div>
                <code className="identity__id" title={identity.userId}>
                  {truncateId(identity.userId)}
                </code>
              </div>
              <CopyButton
                text={identity.userId}
                label={t("identity.copyUserId")}
              />
            </div>
          )}

          {identity.status === "locked" ? (
            <UnlockForm prompt={t("identity.unlockPrompt")} />
          ) : (
            <div className="spread identity__row">
              <p className="muted">{t("identity.unlockedNote")}</p>
              <button
                type="button"
                className="secondary"
                onClick={identity.lock}
              >
                {t("identity.lock")}
              </button>
            </div>
          )}

          <div className="identity__backup">
            <p className="muted">{t("identity.backupWarning")}</p>
            <div className="cluster">
              <button
                type="button"
                className="secondary"
                onClick={() => void downloadBackup()}
              >
                {t("identity.downloadBackup")}
              </button>
              <button
                type="button"
                className="ghost danger-text"
                onClick={() => setForgetting(true)}
              >
                {t("identity.forget")}
              </button>
            </div>
            {backupSaved && (
              <p className="banner ok" role="status">
                {t("identity.backupSaved")}
              </p>
            )}
          </div>
        </div>
      )}

      <ConfirmDialog
        open={forgetting}
        title={t("identity.forgetTitle")}
        confirmLabel={t("identity.forgetConfirm")}
        destructive
        requireText={t("identity.forgetWord")}
        onConfirm={() => {
          setForgetting(false);
          void identity.forget();
        }}
        onCancel={() => setForgetting(false)}
      >
        <p>{t("identity.forgetBody")}</p>
      </ConfirmDialog>
    </section>
  );
}
