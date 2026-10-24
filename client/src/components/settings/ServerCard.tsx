import { useEffect, useState } from "react";
import { errorMessage } from "../../lib/crisp";
import { useIdentity } from "../../lib/identity";
import { createLogger } from "../../lib/log";
import { type ServerStatus, useServer } from "../../lib/servers";
import { Spinner } from "../library/ui";
import { UnlockForm } from "./PassphraseFields";
import { normalizeServerUrl } from "./settingsModel";

const log = createLogger("settings.server");

const STATUS_LABEL: Record<ServerStatus, string> = {
  idle: "Not checked",
  connecting: "Connecting…",
  online: "Online",
  offline: "Offline",
};

/** A colored pill for the connection status. */
export function StatusPill({ status }: { status: ServerStatus }) {
  return (
    <span
      className={`status-pill status-pill--${status}`}
      data-testid="server-status"
      data-status={status}
    >
      <span className="status-pill__dot" aria-hidden="true" />
      {STATUS_LABEL[status]}
    </span>
  );
}

/** Signs in to the server with the unlocked identity. */
function SignIn() {
  const identity = useIdentity();
  const server = useServer();
  const [username, setUsername] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (server.username) {
    return (
      <div className="spread server__row">
        <p>
          Signed in as <strong>{server.username}</strong>
        </p>
        <button
          type="button"
          className="secondary"
          onClick={() => void server.signOut()}
        >
          Sign out
        </button>
      </div>
    );
  }
  if (identity.status === "none" || identity.status === "loading") {
    return (
      <p className="muted">
        Create an identity above to sign in and publish recipes.
      </p>
    );
  }
  if (identity.status === "locked") {
    return (
      <div className="server__unlock">
        <UnlockForm prompt="Unlock your identity to sign in to this server." />
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
        <span>Username</span>
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
        {busy && <Spinner label="Signing in" />}
        Sign in
      </button>
    </form>
  );
}

/** The server card: which server, its status, and the session. */
export function ServerCard() {
  const server = useServer();
  const [url, setUrl] = useState(server.url);
  const [urlError, setUrlError] = useState<string | null>(null);

  useEffect(() => setUrl(server.url), [server.url]);

  const connect = () => {
    const clean = normalizeServerUrl(url);
    if (!clean) {
      setUrlError("Enter an http:// or https:// address.");
      return;
    }
    setUrlError(null);
    if (clean === server.url) void server.refresh();
    else void server.connect(clean);
  };

  return (
    <section className="card settings-card" aria-labelledby="server-title">
      <div className="spread">
        <h2 id="server-title">Server</h2>
        <StatusPill status={server.status} />
      </div>
      <p className="muted">
        Optional. A server lets you publish recipes, browse what others share,
        and get them endorsed. Everything else works offline.
      </p>
      <form
        className="server__url"
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          connect();
        }}
      >
        <label>
          <span>Server address</span>
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
            ? "Retry"
            : "Connect"}
        </button>
      </form>
      {urlError && (
        <p className="field-error" role="alert">
          {urlError}
        </p>
      )}

      {server.status === "offline" && (
        <p className="banner" role="status">
          Can’t reach this server right now
          {server.error ? ` (${server.error})` : ""}. Your library keeps working
          offline.
        </p>
      )}

      {server.keyMismatch && (
        <div className="banner warn" role="alert">
          <p>
            <strong>This server’s signing keys changed.</strong> That can happen
            after a key rotation — or it may not be the server you trusted
            before. Only trust the new keys if you expected this.
          </p>
          <button
            type="button"
            className="secondary"
            onClick={() => void server.trustCurrentKeys()}
          >
            Trust new keys
          </button>
        </div>
      )}

      {server.status === "online" && server.info && (
        <dl className="facts">
          <div>
            <dt>Server core</dt>
            <dd className="tabular">{server.info.core_version}</dd>
          </div>
          <div>
            <dt>Format version</dt>
            <dd className="tabular">
              {server.info.format_version_text}
              {server.info.min_format_version_text !==
              server.info.format_version_text
                ? ` (reads ${server.info.min_format_version_text}+)`
                : ""}
            </dd>
          </div>
        </dl>
      )}

      {(server.status === "online" || server.username) && <SignIn />}
    </section>
  );
}
