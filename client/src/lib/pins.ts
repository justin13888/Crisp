/**
 * The endorsement keys this device pins per server (spec §8.5): reading
 * them back from storage, and applying the engine's pin-update decision.
 */

import type { PinUpdate } from "../generated/crisp";
import type { PlatformKey } from "./crisp";
import { getStore } from "./store";

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
