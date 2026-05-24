/**
 * The identity and server-session context.
 *
 * Holds the unlocked [`CrispIdentity`] (live only in memory — the passphrase
 * is never stored), the active server session, and the configured server
 * URL. Components reach all of this through the [`useIdentity`] hook.
 */

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { ServerClient, type Session } from "./api";
import { type CrispIdentity, deriveIdentity } from "./crisp";

const SERVER_KEY = "crisp.serverUrl";
const DEFAULT_SERVER = "http://localhost:8080";
const USERNAMES_KEY = "crisp.knownUsernames";
const SESSION_KEY = "crisp.session";

/** Reads the remembered (non-secret) usernames from local storage. */
function loadKnownUsernames(): string[] {
  try {
    const raw = localStorage.getItem(USERNAMES_KEY);
    if (raw === null) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed)
      ? parsed.filter((value): value is string => typeof value === "string")
      : [];
  } catch {
    return [];
  }
}

/**
 * Restores a previously persisted server session.
 *
 * Only the session — a bearer token, never the passphrase or key material —
 * is persisted, so a returning user stays signed in across reloads without
 * re-entering credentials. An expired session is dropped rather than restored.
 */
function loadSession(): Session | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    if (raw === null) return null;
    const parsed: unknown = JSON.parse(raw);
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      typeof (parsed as Record<string, unknown>)["token"] === "string" &&
      typeof (parsed as Record<string, unknown>)["usrId"] === "string" &&
      typeof (parsed as Record<string, unknown>)["expiresAt"] === "number"
    ) {
      const session = parsed as Session;
      if (session.expiresAt * 1000 > Date.now()) return session;
    }
    localStorage.removeItem(SESSION_KEY);
    return null;
  } catch {
    return null;
  }
}

/** The value exposed by the identity context. */
export interface IdentityValue {
  /** The unlocked author identity, or `null` when locked. */
  identity: CrispIdentity | null;
  /** The username the identity was unlocked with. */
  username: string | null;
  /** The active server session, or `null` when signed out. */
  session: Session | null;
  /** The configured server base URL. */
  serverUrl: string;
  /** A client bound to the configured server. */
  server: ServerClient;
  /** Usernames an identity has been created or unlocked under on this device.
   * Not secret — used only to tell a returning user apart from a new one. */
  knownUsernames: string[];
  /** Derives and installs an identity from credentials. */
  unlock(username: string, passphrase: string): Promise<CrispIdentity>;
  /** Discards the identity and session, zeroizing key material. */
  lock(): void;
  /** Records (or clears) the active server session, persisting it so the
   * user stays signed in across reloads. */
  setSession(session: Session | null): void;
  /** Changes and persists the server URL. */
  setServerUrl(url: string): void;
  /** Persists `username` as one used on this device. */
  rememberUsername(username: string): void;
}

const IdentityContext = createContext<IdentityValue | null>(null);

/** Provides identity and server state to the app. */
export function IdentityProvider({ children }: { children: ReactNode }) {
  const [identity, setIdentity] = useState<CrispIdentity | null>(null);
  const [username, setUsername] = useState<string | null>(null);
  const [session, setSessionState] = useState<Session | null>(loadSession);
  const [serverUrl, setServerUrlState] = useState<string>(
    () => localStorage.getItem(SERVER_KEY) ?? DEFAULT_SERVER,
  );
  const [knownUsernames, setKnownUsernames] =
    useState<string[]>(loadKnownUsernames);

  const server = useMemo(() => new ServerClient(serverUrl), [serverUrl]);

  // Keep the client's bearer token in step with the active session.
  useEffect(() => {
    server.setToken(session?.token ?? null);
  }, [server, session]);

  const unlock = useCallback(async (name: string, passphrase: string) => {
    const derived = await deriveIdentity(name, passphrase);
    setIdentity((previous) => {
      previous?.dispose();
      return derived;
    });
    setUsername(name);
    return derived;
  }, []);

  // Records the session in both React state and local storage, so it survives
  // a reload; clearing it removes the persisted copy too.
  const setSession = useCallback((next: Session | null) => {
    if (next === null) localStorage.removeItem(SESSION_KEY);
    else localStorage.setItem(SESSION_KEY, JSON.stringify(next));
    setSessionState(next);
  }, []);

  const lock = useCallback(() => {
    setIdentity((previous) => {
      previous?.dispose();
      return null;
    });
    setUsername(null);
    setSession(null);
  }, [setSession]);

  const setServerUrl = useCallback((url: string) => {
    const trimmed = url.trim().replace(/\/$/, "");
    localStorage.setItem(SERVER_KEY, trimmed);
    setServerUrlState(trimmed);
  }, []);

  const rememberUsername = useCallback((name: string) => {
    const trimmed = name.trim();
    if (trimmed === "") return;
    setKnownUsernames((previous) => {
      if (previous.includes(trimmed)) return previous;
      const next = [...previous, trimmed];
      localStorage.setItem(USERNAMES_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const value = useMemo<IdentityValue>(
    () => ({
      identity,
      username,
      session,
      serverUrl,
      server,
      knownUsernames,
      unlock,
      lock,
      setSession,
      setServerUrl,
      rememberUsername,
    }),
    [
      identity,
      username,
      session,
      serverUrl,
      server,
      knownUsernames,
      unlock,
      lock,
      setSession,
      setServerUrl,
      rememberUsername,
    ],
  );

  return (
    <IdentityContext.Provider value={value}>
      {children}
    </IdentityContext.Provider>
  );
}

/** Reads the identity context; throws if used outside the provider. */
export function useIdentity(): IdentityValue {
  const context = useContext(IdentityContext);
  if (!context) {
    throw new Error("useIdentity must be used within an IdentityProvider");
  }
  return context;
}
