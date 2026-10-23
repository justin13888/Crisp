/**
 * The connection to a Crisp server: its pinned endorsement keys, the
 * signed-in session, and publishing. Everything works offline without one;
 * a server only adds publishing, browsing, and endorsement.
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
import {
  type CrispServer,
  errorMessage,
  isCrispFailure,
  loadEngine,
  nowSeconds,
  type PlatformKey,
  type ServerInfoView,
} from "./crisp";
import { useIdentity } from "./identity";
import { createLogger } from "./log";
import { getStore, type ServerRecord } from "./store";

const log = createLogger("server");

/** The default server, used until the cook picks another. */
export const DEFAULT_SERVER_URL = "http://localhost:8080";

/** The local setting naming the active server. */
const ACTIVE_KEY = "crisp.activeServer";

/** Narrows stored platform keys (written by us, re-checked on read). */
export function isPlatformKey(value: unknown): value is PlatformKey {
  if (typeof value !== "object" || value === null) return false;
  const key = value as Record<string, unknown>;
  const keyset = key["keyset"];
  if (typeof keyset !== "object" || keyset === null) return false;
  const set = keyset as Record<string, unknown>;
  return (
    set["ed25519"] instanceof Uint8Array &&
    set["ml-dsa-65"] instanceof Uint8Array &&
    typeof key["valid_from"] === "number"
  );
}

/** Every endorsement key this device has pinned, across servers. */
export async function trustedKeys(): Promise<PlatformKey[]> {
  const store = await getStore();
  const records = await store.servers.all();
  return records.flatMap((record) =>
    Array.isArray(record.platformKeys)
      ? record.platformKeys.filter(isPlatformKey)
      : [],
  );
}

/** Whether two key lists name the same keys. */
function sameKeys(a: readonly PlatformKey[], b: readonly PlatformKey[]) {
  const id = (k: PlatformKey) =>
    Array.from(k.keyset.ed25519, (x) => x.toString(16)).join("");
  return (
    a.length === b.length &&
    a.every((k, i) => id(k) === id(b[i] as PlatformKey))
  );
}

/** Where the connection stands. */
export type ServerStatus = "idle" | "connecting" | "online" | "offline";

/** Everything the server context offers. */
export interface ServerContextValue {
  /** The active server's base URL. */
  url: string;
  /** Connection status. */
  status: ServerStatus;
  /** The last server info read, when online. */
  info: ServerInfoView | null;
  /** The last connection problem. */
  error: string | null;
  /** Set when the server's keys differ from the ones pinned earlier. */
  keyMismatch: boolean;
  /** The signed-in username, if any. */
  username: string | null;
  /** Switches to (and probes) another server. */
  connect(url: string): Promise<void>;
  /** Re-probes the active server. */
  refresh(): Promise<void>;
  /** Pins the server's current keys after a mismatch. */
  trustCurrentKeys(): Promise<void>;
  /** Registers (if needed) and signs in with the unlocked identity. */
  signIn(username: string): Promise<void>;
  /** Ends the session. */
  signOut(): Promise<void>;
  /** Publishes an author-signed file; returns the endorsed bytes. */
  publish(bundle: Uint8Array): Promise<Uint8Array>;
  /** A client for the active server, carrying the session. */
  client(): Promise<CrispServer>;
}

const ServerContext = createContext<ServerContextValue | null>(null);

/** Provides the server connection to the app. */
export function ServerProvider({ children }: { children: ReactNode }) {
  const identity = useIdentity();
  const [url, setUrl] = useState(
    () => localStorage.getItem(ACTIVE_KEY) ?? DEFAULT_SERVER_URL,
  );
  const [record, setRecord] = useState<ServerRecord | null>(null);
  const [status, setStatus] = useState<ServerStatus>("idle");
  const [info, setInfo] = useState<ServerInfoView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [keyMismatch, setKeyMismatch] = useState(false);

  const client = useCallback(async () => {
    const engine = await loadEngine();
    const server = new engine.CrispServer(url);
    server.setToken(record?.token ?? null);
    return server;
  }, [url, record]);

  const probe = useCallback(async (target: string) => {
    setStatus("connecting");
    setError(null);
    const store = await getStore();
    const stored = (await store.servers.get(target)) ?? null;
    setRecord(stored);
    try {
      const engine = await loadEngine();
      const fresh = await new engine.CrispServer(target).info();
      const pinned = Array.isArray(stored?.platformKeys)
        ? stored.platformKeys.filter(isPlatformKey)
        : [];
      if (pinned.length > 0 && !sameKeys(pinned, fresh.platform_keys)) {
        log.warn("keys.mismatch", { url: target });
        setKeyMismatch(true);
      } else {
        setKeyMismatch(false);
        const next: ServerRecord = {
          ...(stored ?? {}),
          url: target,
          platformKeys: fresh.platform_keys,
          updatedAt: Date.now(),
        };
        await store.servers.put(next);
        setRecord(next);
      }
      setInfo(fresh);
      setStatus("online");
      log.info("online", { url: target, core: fresh.core_version });
    } catch (cause: unknown) {
      setInfo(null);
      setStatus("offline");
      setError(errorMessage(cause));
      log.info("offline", { url: target, error: errorMessage(cause) });
    }
  }, []);

  useEffect(() => {
    void probe(url);
  }, [probe, url]);

  const connect = useCallback(async (next: string) => {
    const clean = next.trim().replace(/\/+$/, "");
    localStorage.setItem(ACTIVE_KEY, clean);
    setUrl(clean);
  }, []);

  const refresh = useCallback(() => probe(url), [probe, url]);

  const trustCurrentKeys = useCallback(async () => {
    if (!info) return;
    const store = await getStore();
    const next: ServerRecord = {
      ...(record ?? {}),
      url,
      platformKeys: info.platform_keys,
      updatedAt: Date.now(),
    };
    await store.servers.put(next);
    setRecord(next);
    setKeyMismatch(false);
  }, [info, record, url]);

  const saveSession = useCallback(
    async (token: string | undefined, username: string | undefined) => {
      const store = await getStore();
      const base: ServerRecord = record ?? {
        url,
        platformKeys: info?.platform_keys ?? [],
        updatedAt: Date.now(),
      };
      const next: ServerRecord = { ...base, updatedAt: Date.now() };
      if (token && username) {
        next.token = token;
        next.username = username;
      } else {
        delete next.token;
        delete next.username;
      }
      await store.servers.put(next);
      setRecord(next);
    },
    [info, record, url],
  );

  const signIn = useCallback(
    async (username: string) => {
      const signer = identity.signer;
      if (!signer || !identity.displayName) {
        throw new Error("Unlock your identity first.");
      }
      const engine = await loadEngine();
      const server = new engine.CrispServer(url);
      const profile = signer.signProfile(
        username,
        identity.displayName,
        nowSeconds(),
      );
      try {
        await server.register(profile);
      } catch (cause: unknown) {
        // 409: this key or name is already registered — signing in decides.
        if (!(isCrispFailure(cause) && cause.status === 409)) throw cause;
      }
      const challenge = await server.challenge(username);
      const signature = signer.signLogin(challenge.audience, challenge.nonce);
      const session = await server.login(challenge.nonce, signature);
      await saveSession(session.token, username);
      log.info("signed-in", { url, userId: session.user_id });
    },
    [identity.displayName, identity.signer, saveSession, url],
  );

  const signOut = useCallback(async () => {
    try {
      await (await client()).logout();
    } catch (cause: unknown) {
      log.warn("logout.failed", { error: errorMessage(cause) });
    }
    await saveSession(undefined, undefined);
  }, [client, saveSession]);

  const publish = useCallback(
    async (bundle: Uint8Array) => (await client()).publish(bundle),
    [client],
  );

  const value = useMemo<ServerContextValue>(
    () => ({
      url,
      status,
      info,
      error,
      keyMismatch,
      username: record?.username ?? null,
      connect,
      refresh,
      trustCurrentKeys,
      signIn,
      signOut,
      publish,
      client,
    }),
    [
      url,
      status,
      info,
      error,
      keyMismatch,
      record,
      connect,
      refresh,
      trustCurrentKeys,
      signIn,
      signOut,
      publish,
      client,
    ],
  );
  return (
    <ServerContext.Provider value={value}>{children}</ServerContext.Provider>
  );
}

/** The server context. */
export function useServer(): ServerContextValue {
  const value = useContext(ServerContext);
  if (!value) throw new Error("useServer needs a <ServerProvider>");
  return value;
}
