/**
 * What this device trusts per server: the endorsement keys it pins
 * (spec §8.5) and the endorsement revocations it holds (spec §8.8) —
 * reading them back from storage, and applying the engine's decisions.
 */

import type { PinUpdate } from "../generated/crisp";
import type { PlatformKey, Revocation, RevocationsView } from "./crisp";
import { errorMessage } from "./crisp";
import { isRecord } from "./isRecord";
import { createLogger } from "./log";
import { getStore, type ServerRecord } from "./store";

const log = createLogger("pins");

/** Narrows stored platform keys (written by us, re-checked on read). */
export function isPlatformKey(value: unknown): value is PlatformKey {
  if (typeof value !== "object" || value === null) return false;
  const key = value as Record<string, unknown>;
  const keyset = key["keyset"];
  if (typeof keyset !== "object" || keyset === null) return false;
  const set = keyset as Record<string, unknown>;
  const until = key["valid_until"];
  return (
    set["ed25519"] instanceof Uint8Array &&
    set["ml-dsa-65"] instanceof Uint8Array &&
    typeof key["valid_from"] === "number" &&
    (until === undefined || until === null || typeof until === "number")
  );
}

/** Narrows a stored revocation (written by us, re-checked on read). */
export function isRevocation(value: unknown): value is Revocation {
  return (
    isRecord(value) &&
    value["content_hash"] instanceof Uint8Array &&
    typeof value["revoked_at"] === "number"
  );
}

/** The keys a stored server record pins, narrowed. */
export function pinnedKeys(
  record: ServerRecord | null | undefined,
): PlatformKey[] {
  const keys = record?.platformKeys;
  return Array.isArray(keys) ? keys.filter(isPlatformKey) : [];
}

/** The revocations a stored server record holds, narrowed. */
export function heldRevocations(
  record: ServerRecord | null | undefined,
): Revocation[] {
  const held = record?.revocations;
  return Array.isArray(held) ? held.filter(isRevocation) : [];
}

/** One server's trust: the keys pinned for it and its held revocations. */
export interface TrustedServer {
  /** The endorsement keys pinned for the server. */
  keys: PlatformKey[];
  /** The revocations accepted from the server. */
  revocations: Revocation[];
}

/** What this device trusts, per known server. */
export async function trustedServers(): Promise<TrustedServer[]> {
  const store = await getStore();
  const records = await store.servers.all();
  return records.map((record) => ({
    keys: pinnedKeys(record),
    revocations: heldRevocations(record),
  }));
}

/**
 * The keys to pin after `updatePin` decided `outcome` for a server whose
 * pin was `pinned`, or `null` when the change was refused and the cook must
 * be told the server's keys changed (spec §8.5).
 */
export function pinAfter(
  pinned: PlatformKey[],
  outcome: PinUpdate,
): PlatformKey[] | null {
  switch (outcome.kind) {
    case "unchanged":
      return pinned;
    case "updated":
      return outcome.keys;
    case "rejected":
      return null;
  }
}

/**
 * The revocations to hold for a server after offering it the chance to add
 * to `held` (spec §8.8): `fetchList` reads the server's signed list and
 * `accept` (the engine's `acceptRevocations`) checks it against `pinned`.
 * Any failure — the server unreachable, the list unsigned by a current
 * pinned key — keeps `held`, and so does a result shorter than `held`:
 * revocations are permanent.
 */
export async function revocationsAfter(
  url: string,
  pinned: PlatformKey[],
  held: Revocation[],
  fetchList: () => Promise<RevocationsView>,
  accept: (
    pinned: PlatformKey[],
    held: Revocation[],
    offered: RevocationsView,
  ) => Revocation[],
): Promise<Revocation[]> {
  try {
    const offered = await fetchList();
    const next = accept(pinned, held, offered);
    if (next.length < held.length) {
      log.warn("revocations.shrank", {
        url,
        held: held.length,
        offered: next.length,
      });
      return held;
    }
    if (next.length > held.length) {
      log.info("revocations.accepted", {
        url,
        held: held.length,
        now: next.length,
      });
    } else {
      log.debug("revocations.unchanged", { url, held: held.length });
    }
    return next;
  } catch (cause: unknown) {
    log.warn("revocations.refused", {
      url,
      held: held.length,
      error: errorMessage(cause),
    });
    return held;
  }
}
