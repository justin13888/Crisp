import { useEffect, useState } from "react";
import { LITERAL_VALUES, Trans, useTranslation } from "../../i18n";
import { errorMessage } from "../../lib/crisp";
import { useIdentity } from "../../lib/identity";
import { createLogger } from "../../lib/log";
import { type ServerStatus, useServer } from "../../lib/servers";
import { Spinner } from "../library/ui";
import { UnlockForm } from "./PassphraseFields";
import { normalizeServerUrl } from "./settingsModel";

const log = createLogger("settings.server");

/** The catalogue key naming each connection status. */
const STATUS_LABEL = {
  idle: "server.status.idle",
  connecting: "server.status.connecting",
  online: "server.status.online",
  offline: "server.status.offline",
} as const satisfies Record<ServerStatus, `server.status.${ServerStatus}`>;

/** A colored pill for the connection status. */
export function StatusPill({ status }: { status: ServerStatus }) {
  const { t } = useTranslation("settings");
  return (
    <span
      className={`status-pill status-pill--${status}`}
      data-testid="server-status"
      data-status={status}
    >
      <span className="status-pill__dot" aria-hidden="true" />
      {t(STATUS_LABEL[status])}
    </span>
  );
}

/** Signs in to the server with the unlocked identity. */
function SignIn() {
  const { t } = useTranslation("settings");
  const identity = useIdentity();
  const server = useServer();
  const [username, setUsername] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (server.username) {
    return (
      <div className="spread server__row">
        <p>
          <Trans
            t={t}
            i18nKey="server.signedInAs"
            {...LITERAL_VALUES}
            values={{ username: server.username }}
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
    );
  }
  if (identity.status === "none" || identity.status === "loading") {
    return <p className="muted">{t("server.needIdentity")}</p>;
  }
  if (identity.status === "locked") {
    return (
      <div className="server__unlock">
        <UnlockForm prompt={t("server.unlockPrompt")} />
      </div>
    );
  }

  const submit = async () => {
    const name = username.trim();
    if (!name) return;
    setBusy(true);
    setError(null);
    try {
      await server.signIn(name);
      setUsername("");
    } catch (cause: unknown) {
      log.warn("signin.failed", { error: errorMessage(cause) });
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="server__signin"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <label>
        <span>{t("server.username")}</span>
        <input
          value={username}
          onChange={(event) => setUsername(event.target.value)}
          autoComplete="username"
          autoCapitalize="off"
          spellCheck={false}
        />
      </label>
      {error && (
        <p className="field-error" role="alert">
          {error}
        </p>
      )}
      <button type="submit" disabled={busy || !username.trim()}>
        {busy && <Spinner label={t("server.signingIn")} />}
        {t("server.signIn")}
      </button>
    </form>
  );
}

/** The server card: which server, its status, and the session. */
export function ServerCard() {
  const { t } = useTranslation("settings");
  const server = useServer();
  const [url, setUrl] = useState(server.url);
  const [urlError, setUrlError] = useState<string | null>(null);

  useEffect(() => setUrl(server.url), [server.url]);

  const connect = () => {
    const clean = normalizeServerUrl(url);
    if (!clean) {
      setUrlError(t("server.badAddress"));
      return;
    }
    setUrlError(null);
    if (clean === server.url) void server.refresh();
    else void server.connect(clean);
  };

  return (
    <section className="card settings-card" aria-labelledby="server-title">
      <div className="spread">
        <h2 id="server-title">{t("server.title")}</h2>
        <StatusPill status={server.status} />
      </div>
      <p className="muted">{t("server.intro")}</p>
      <form
        className="server__url"
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          connect();
        }}
      >
        <label>
          <span>{t("server.address")}</span>
          <input
            type="url"
            inputMode="url"
            value={url}
            onChange={(event) => setUrl(event.target.value)}
            autoCapitalize="off"
            spellCheck={false}
            aria-invalid={urlError ? true : undefined}
          />
        </label>
        <button
          type="submit"
          className="secondary"
          disabled={server.status === "connecting"}
        >
          {url.trim() && normalizeServerUrl(url) === server.url
            ? t("server.retry")
            : t("server.connect")}
        </button>
      </form>
      {urlError && (
        <p className="field-error" role="alert">
          {urlError}
        </p>
      )}

      {server.status === "offline" && (
        <p className="banner" role="status">
          {server.error
            ? t("server.offlineWithError", { error: server.error })
            : t("server.offline")}
        </p>
      )}

      {server.keyMismatch && (
        <div className="banner warn" role="alert">
          <p>
            <Trans
              t={t}
              i18nKey="server.keyMismatch"
              components={{ strong: <strong /> }}
            />
          </p>
          <button
            type="button"
            className="secondary"
            onClick={() => void server.trustCurrentKeys()}
          >
            {t("server.trustKeys")}
          </button>
        </div>
      )}

      {server.status === "online" && server.info && (
        <dl className="facts">
          <div>
            <dt>{t("server.core")}</dt>
            <dd className="tabular">{server.info.core_version}</dd>
          </div>
          <div>
            <dt>{t("server.formatVersion")}</dt>
            <dd className="tabular">
              {server.info.min_format_version_text !==
              server.info.format_version_text
                ? t("version.reads", {
                    version: server.info.format_version_text,
                    min: server.info.min_format_version_text,
                  })
                : server.info.format_version_text}
            </dd>
          </div>
        </dl>
      )}

      {(server.status === "online" || server.username) && <SignIn />}
    </section>
  );
}
