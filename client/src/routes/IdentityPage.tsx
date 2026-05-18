import { type FormEvent, useEffect, useState } from "react";
import type { ServerInfo } from "../lib/api";
import { formatVersion } from "../lib/crisp";
import { useIdentity } from "../lib/identity";

/** Identity, server connection, and account management. */
export function IdentityPage() {
  const {
    identity,
    username,
    session,
    server,
    serverUrl,
    unlock,
    lock,
    setSession,
    setServerUrl,
  } = useIdentity();

  const [usernameInput, setUsernameInput] = useState(username ?? "");
  const [passphrase, setPassphrase] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [urlInput, setUrlInput] = useState(serverUrl);
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const [clientVersion, setClientVersion] = useState<number | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    formatVersion()
      .then(setClientVersion)
      .catch(() => setClientVersion(null));
  }, []);

  useEffect(() => {
    setInfo(null);
    server
      .serverInfo()
      .then(setInfo)
      .catch(() => setInfo(null));
  }, [server]);

  const drift =
    info && clientVersion !== null
      ? clientVersion < info.format_version.min_supported ||
        clientVersion > info.format_version.max_supported
      : false;

  const run = async (label: string, action: () => Promise<string>) => {
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      setStatus(await action());
    } catch (e: unknown) {
      setError(`${label}: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const onUnlock = (event: FormEvent) => {
    event.preventDefault();
    void run("Unlock", async () => {
      const derived = await unlock(usernameInput.trim(), passphrase);
      setPassphrase("");
      return `Identity unlocked: ${derived.userId}`;
    });
  };

  const onRegister = () => {
    if (!identity) return;
    void run("Register", async () => {
      const usrId = await server.register(
        usernameInput.trim(),
        displayName.trim(),
        identity,
      );
      return `Registered account ${usrId}`;
    });
  };

  const onLogin = () => {
    if (!identity) return;
    void run("Sign in", async () => {
      const newSession = await server.login(usernameInput.trim(), identity);
      setSession(newSession);
      return `Signed in as ${newSession.usrId}`;
    });
  };

  const onLogout = () => {
    void run("Sign out", async () => {
      await server.logout();
      setSession(null);
      return "Signed out.";
    });
  };

  return (
    <section>
      <h1>Identity &amp; account</h1>

      <div className="card">
        <h2>Server</h2>
        <div className="row">
          <label style={{ flex: 1 }}>
            <span>Server URL</span>
            <input
              value={urlInput}
              onChange={(e) => setUrlInput(e.target.value)}
            />
          </label>
          <button type="button" onClick={() => setServerUrl(urlInput)}>
            Connect
          </button>
        </div>
        {info ? (
          <p className="muted">
            Connected · core {info.core_version} · format{" "}
            {info.format_version.current}
          </p>
        ) : (
          <p className="muted">Not connected to this server.</p>
        )}
        {drift && (
          <div className="banner warn">
            Version drift: this app speaks format {clientVersion}, but the
            server supports {info?.format_version.min_supported}–
            {info?.format_version.max_supported}. Some bundles may not
            interoperate — consider updating.
          </div>
        )}
      </div>

      <div className="card">
        <h2>Your identity</h2>
        <p className="muted">
          Your key is derived from your username and passphrase on this device.
          The passphrase is never sent anywhere.
        </p>
        {identity ? (
          <>
            <p>
              <strong>Unlocked:</strong> <code>{identity.userId}</code>
            </p>
            <button type="button" className="secondary" onClick={lock}>
              Lock identity
            </button>
          </>
        ) : (
          <form onSubmit={onUnlock}>
            <label>
              <span>Username</span>
              <input
                value={usernameInput}
                onChange={(e) => setUsernameInput(e.target.value)}
              />
            </label>
            <label>
              <span>Passphrase</span>
              <input
                type="password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
              />
            </label>
            <button
              type="submit"
              disabled={busy || !usernameInput || !passphrase}
            >
              Unlock identity
            </button>
          </form>
        )}
      </div>

      {identity && (
        <div className="card">
          <h2>Account</h2>
          {session ? (
            <>
              <p>
                Signed in as <code>{session.usrId}</code>.
              </p>
              <button
                type="button"
                className="secondary"
                onClick={onLogout}
                disabled={busy}
              >
                Sign out
              </button>
            </>
          ) : (
            <>
              <label>
                <span>Display name (for registration)</span>
                <input
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                />
              </label>
              <div className="row">
                <button
                  type="button"
                  onClick={onRegister}
                  disabled={busy || !displayName}
                >
                  Register
                </button>
                <button
                  type="button"
                  className="secondary"
                  onClick={onLogin}
                  disabled={busy}
                >
                  Sign in
                </button>
              </div>
            </>
          )}
        </div>
      )}

      {status && (
        <div className="banner" style={{ background: "#e3f2e8" }}>
          {status}
        </div>
      )}
      {error && <div className="banner bad">{error}</div>}
    </section>
  );
}
