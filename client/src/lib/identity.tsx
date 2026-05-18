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
  /** Derives and installs an identity from credentials. */
  unlock(username: string, passphrase: string): Promise<CrispIdentity>;
  /** Discards the identity and session, zeroizing key material. */
  lock(): void;
  /** Records (or clears) the active server session. */
  setSession(session: Session | null): void;
  /** Changes and persists the server URL. */
  setServerUrl(url: string): void;
}

const IdentityContext = createContext<IdentityValue | null>(null);

/** Provides identity and server state to the app. */
export function IdentityProvider({ children }: { children: ReactNode }) {
  const [identity, setIdentity] = useState<CrispIdentity | null>(null);
  const [username, setUsername] = useState<string | null>(null);
  const [session, setSession] = useState<Session | null>(null);
  const [serverUrl, setServerUrlState] = useState<string>(
    () => localStorage.getItem(SERVER_KEY) ?? DEFAULT_SERVER,
  );

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

  const lock = useCallback(() => {
    setIdentity((previous) => {
      previous?.dispose();
      return null;
    });
    setUsername(null);
    setSession(null);
  }, []);

  const setServerUrl = useCallback((url: string) => {
    const trimmed = url.trim().replace(/\/$/, "");
    localStorage.setItem(SERVER_KEY, trimmed);
    setServerUrlState(trimmed);
  }, []);

  const value = useMemo<IdentityValue>(
    () => ({
      identity,
      username,
      session,
      serverUrl,
      server,
      unlock,
      lock,
      setSession,
      setServerUrl,
    }),
    [
      identity,
      username,
      session,
      serverUrl,
      server,
      unlock,
      lock,
      setServerUrl,
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
