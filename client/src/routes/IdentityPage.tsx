import { type FormEvent, useCallback, useEffect, useState } from "react";
import type { ServerInfo } from "../lib/api";
import { formatVersion } from "../lib/crisp";
import { useIdentity } from "../lib/identity";

/** The two things a locked user can do: start fresh, or return. */
type Mode = "create" | "unlock";

/** Identity, server connection, and account management. */
export function IdentityPage() {
  const {
    identity,
    username,
    session,
    server,
    serverUrl,
    knownUsernames,
    unlock,
    lock,
    setSession,
    setServerUrl,
    rememberUsername,
  } = useIdentity();

  const [usernameInput, setUsernameInput] = useState(
    () => username ?? knownUsernames[knownUsernames.length - 1] ?? "",
  );
  const [mode, setMode] = useState<Mode>(() => {
    const initial = (
      username ??
      knownUsernames[knownUsernames.length - 1] ??
      ""
    ).trim();
    return initial !== "" && knownUsernames.includes(initial)
      ? "unlock"
      : "create";
  });
  const [passphrase, setPassphrase] = useState("");
  const [confirmPassphrase, setConfirmPassphrase] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [urlInput, setUrlInput] = useState(serverUrl);
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const [clientVersion, setClientVersion] = useState<number | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [connError, setConnError] = useState<string | null>(null);

  useEffect(() => {
    formatVersion()
      .then(setClientVersion)
      .catch(() => setClientVersion(null));
  }, []);

  // Probes the configured server, surfacing in-flight and failure states.
  const checkConnection = useCallback(async () => {
    setConnecting(true);
    setConnError(null);
    setInfo(null);
    try {
      setInfo(await server.serverInfo());
    } catch (e: unknown) {
      setConnError(e instanceof Error ? e.message : String(e));
    } finally {
      setConnecting(false);
    }
  }, [server]);

  useEffect(() => {
    void checkConnection();
  }, [checkConnection]);

  const normalizedInput = urlInput.trim().replace(/\/$/, "");
  const urlChanged = normalizedInput !== serverUrl;

  // A new URL swaps the client (re-probing via effect); the same URL re-probes
  // directly, since the memoized client would not otherwise change.
  const onConnect = () => {
    if (urlChanged) setServerUrl(urlInput);
    else void checkConnection();
  };

  let connectLabel: string;
  if (connecting) connectLabel = "Connecting…";
  else if (urlChanged) connectLabel = "Connect";
  else if (info) connectLabel = "Reconnect";
  else connectLabel = "Retry";

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

  // Switching the mode is always available — the user, not a heuristic, has
  // the final say on whether they are new here or returning.
  const switchMode = (next: Mode) => {
    setMode(next);
    setConfirmPassphrase("");
    setError(null);
  };

  const trimmedName = usernameInput.trim();
  // What this device remembers about the typed username — used only to nudge
  // the user toward the matching mode, never to override their choice.
  const detected: Mode =
    trimmedName !== "" && knownUsernames.includes(trimmedName)
      ? "unlock"
      : "create";
  const modeHint =
    trimmedName === "" || mode === detected
      ? null
      : detected === "unlock"
        ? `You’ve used “${trimmedName}” on this device before — “I already have an identity” will unlock it.`
        : `“${trimmedName}” hasn’t been used on this device. If it’s a new identity, choose “Create a new identity”.`;

  const confirmMismatch =
    mode === "create" &&
    confirmPassphrase.length > 0 &&
    passphrase !== confirmPassphrase;

  const submitDisabled =
    busy ||
    trimmedName === "" ||
    passphrase === "" ||
    (mode === "create" &&
      (displayName.trim() === "" || passphrase !== confirmPassphrase));

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    const name = trimmedName;
    setBusy(true);
    setError(null);
    setStatus(null);
    void (async () => {
      try {
        const derived = await unlock(name, passphrase);
        if (mode === "create") {
          try {
            const usrId = await server.register(
              derived,
              name,
              displayName.trim(),
            );
            setSession(await server.login(name, derived));
            rememberUsername(name);
            setStatus(`Identity created and registered: ${usrId}`);
          } catch (e: unknown) {
            const message = e instanceof Error ? e.message : String(e);
            // A deterministic re-derivation of an existing identity (a
            // returning user on a new device): sign in rather than fail.
            if (message.includes("already registered")) {
              setSession(await server.login(name, derived));
              rememberUsername(name);
              setMode("unlock");
              setStatus(
                `This identity already exists — signed in as ${derived.userId}`,
              );
            } else {
              throw new Error(`could not register this identity: ${message}`);
            }
          }
        } else {
          rememberUsername(name);
          try {
            const newSession = await server.login(name, derived);
            setSession(newSession);
            setStatus(`Signed in as ${newSession.usrId}`);
          } catch (e: unknown) {
            const message = e instanceof Error ? e.message : String(e);
            // The identity is unlocked locally regardless; only sign-in needs
            // the server, so a connection failure is not fatal here.
            setStatus(
              `Identity unlocked: ${derived.userId} — sign-in unavailable (${message})`,
            );
          }
        }
        setPassphrase("");
        setConfirmPassphrase("");
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    })();
  };

  const onSignIn = () => {
    if (!identity || username === null) return;
    void run("Sign in", async () => {
      const newSession = await server.login(username, identity);
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
          <button
            type="button"
            onClick={onConnect}
            disabled={connecting || !normalizedInput}
            aria-busy={connecting}
          >
            {connectLabel}
          </button>
        </div>
        {connecting ? (
          <p className="muted">Connecting to {serverUrl}…</p>
        ) : info ? (
          <p className="muted">
            Connected to {serverUrl} · core {info.core_version} · format{" "}
            {info.format_version.current}
          </p>
        ) : connError ? (
          <div className="banner bad">
            Couldn’t connect to {serverUrl}: {connError}
          </div>
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
        {identity ? (
          <>
            <p className="muted">
              Your key is derived from your username and passphrase on this
              device. The passphrase is never sent or stored anywhere.
            </p>
            <p>
              <strong>Unlocked:</strong> <code>{identity.userId}</code>
            </p>
            <button type="button" className="secondary" onClick={lock}>
              Lock identity
            </button>
          </>
        ) : (
          <>
            {session && (
              <p className="muted">
                You’re signed in as <code>{session.usrId}</code>, but your
                identity is locked on this device. Unlock it below to publish or
                author recipes.
              </p>
            )}
            <div
              className="row"
              role="tablist"
              aria-label="Create or unlock an identity"
            >
              <button
                type="button"
                role="tab"
                aria-selected={mode === "create"}
                className={mode === "create" ? undefined : "secondary"}
                onClick={() => switchMode("create")}
              >
                Create a new identity
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={mode === "unlock"}
                className={mode === "unlock" ? undefined : "secondary"}
                onClick={() => switchMode("unlock")}
              >
                I already have an identity
              </button>
            </div>
            <p className="muted">
              {mode === "create"
                ? "Pick a username and passphrase — your key is derived from them on this device. The passphrase is never sent or stored anywhere."
                : "Enter the username and passphrase you chose when you created your identity, on any device, to unlock it here."}
            </p>
            <form onSubmit={onSubmit}>
              <label>
                <span>Username</span>
                <input
                  value={usernameInput}
                  autoComplete="username"
                  onChange={(e) => setUsernameInput(e.target.value)}
                />
              </label>
              {mode === "create" && (
                <label>
                  <span>Display name</span>
                  <input
                    value={displayName}
                    onChange={(e) => setDisplayName(e.target.value)}
                  />
                </label>
              )}
              <label>
                <span>Passphrase</span>
                <input
                  type="password"
                  value={passphrase}
                  autoComplete={
                    mode === "create" ? "new-password" : "current-password"
                  }
                  onChange={(e) => setPassphrase(e.target.value)}
                />
              </label>
              {mode === "create" && (
                <label>
                  <span>Confirm passphrase</span>
                  <input
                    type="password"
                    value={confirmPassphrase}
                    autoComplete="new-password"
                    onChange={(e) => setConfirmPassphrase(e.target.value)}
                  />
                </label>
              )}
              {confirmMismatch && (
                <p className="muted">The passphrases do not match yet.</p>
              )}
              {modeHint && <p className="muted">{modeHint}</p>}
              {mode === "create" && (
                <div className="banner warn">
                  Your passphrase <strong>is</strong> your key — it can’t be
                  reset or recovered. If you mistype it, you’ll get a different,
                  permanent identity with no way back. Choose a username no one
                  else has used and store the passphrase somewhere safe.
                </div>
              )}
              <button type="submit" disabled={submitDisabled}>
                {mode === "create" ? "Create identity" : "Unlock & sign in"}
              </button>
            </form>
          </>
        )}
      </div>

      {(identity || session) && (
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
              <p className="muted">
                This identity is unlocked but not signed in to {serverUrl}.
              </p>
              <button type="button" onClick={onSignIn} disabled={busy}>
                Sign in
              </button>
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
