import { describe, expect, test } from "bun:test";
import type {
  PlatformKey,
  Revocation,
  RevocationsView,
} from "../generated/crisp";
import {
  heldRevocations,
  isPlatformKey,
  isRevocation,
  pinAfter,
  pinnedKeys,
  revocationsAfter,
} from "./pins";

// Pinned platform keys come back from IndexedDB untrusted; only a value
// with both public keys as bytes and a numeric `valid_from` is a key.

function key(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    keyset: {
      ed25519: new Uint8Array(32),
      "ml-dsa-65": new Uint8Array(1952),
    },
    valid_from: 1_700_000_000,
    ...overrides,
  };
}

describe("isPlatformKey", () => {
  test("accepts a stored platform key", () => {
    expect(isPlatformKey(key())).toBe(true);
    expect(isPlatformKey(key({ valid_until: 1_800_000_000 }))).toBe(true);
  });

  test("rejects values that are not objects", () => {
    for (const value of [null, undefined, 0, "key", true, []]) {
      expect(isPlatformKey(value)).toBe(false);
    }
  });

  test("rejects a missing or non-object keyset", () => {
    expect(isPlatformKey(key({ keyset: undefined }))).toBe(false);
    expect(isPlatformKey(key({ keyset: null }))).toBe(false);
    expect(isPlatformKey(key({ keyset: "ed25519" }))).toBe(false);
  });

  test("rejects public keys that are not bytes", () => {
    const ml = new Uint8Array(1952);
    expect(
      isPlatformKey(key({ keyset: { ed25519: [1, 2, 3], "ml-dsa-65": ml } })),
    ).toBe(false);
    expect(
      isPlatformKey(
        key({
          keyset: { ed25519: new Uint8Array(32).buffer, "ml-dsa-65": ml },
        }),
      ),
    ).toBe(false);
    expect(
      isPlatformKey(key({ keyset: { ed25519: new Uint8Array(32) } })),
    ).toBe(false);
  });

  test("rejects a missing or non-numeric valid_from", () => {
    expect(isPlatformKey(key({ valid_from: undefined }))).toBe(false);
    expect(isPlatformKey(key({ valid_from: "1700000000" }))).toBe(false);
  });

  test("rejects a non-numeric valid_until", () => {
    expect(isPlatformKey(key({ valid_until: null }))).toBe(true);
    expect(isPlatformKey(key({ valid_until: "1800000000" }))).toBe(false);
  });
});

describe("pinAfter", () => {
  const pinned = [key() as unknown as PlatformKey];
  const rotated = [
    key({ valid_from: 1_800_000_000 }) as unknown as PlatformKey,
    key({ valid_until: 1_800_000_000 }) as unknown as PlatformKey,
  ];

  test("an unchanged set keeps the pin it had", () => {
    expect(pinAfter(pinned, { kind: "unchanged" })).toBe(pinned);
  });

  test("a vouched change pins the keys the engine returned", () => {
    expect(pinAfter(pinned, { kind: "updated", keys: rotated })).toBe(rotated);
  });

  test("a refused change pins nothing, so the cook is warned", () => {
    expect(pinAfter(pinned, { kind: "rejected", reason: "unsigned" })).toBe(
      null,
    );
  });
});

describe("isRevocation", () => {
  const revocation = { content_hash: new Uint8Array(32), revoked_at: 1 };

  test("accepts a stored revocation", () => {
    expect(isRevocation(revocation)).toBe(true);
  });

  test("rejects anything else", () => {
    for (const value of [
      null,
      [],
      "x",
      { ...revocation, content_hash: [1, 2] },
      { ...revocation, revoked_at: "1" },
      { content_hash: new Uint8Array(32) },
    ]) {
      expect(isRevocation(value)).toBe(false);
    }
  });
});

describe("pinnedKeys and heldRevocations", () => {
  const revocation = { content_hash: new Uint8Array(32), revoked_at: 1 };

  test("narrow what a stored record holds, dropping malformed entries", () => {
    const record = {
      url: "https://a.example",
      platformKeys: [key(), { keyset: null }],
      revocations: [revocation, { content_hash: "ab" }],
      updatedAt: 0,
    };
    expect(pinnedKeys(record)).toHaveLength(1);
    expect(heldRevocations(record)).toEqual([revocation]);
  });

  test("hold nothing for a missing record or a non-array field", () => {
    expect(pinnedKeys(null)).toEqual([]);
    expect(heldRevocations(undefined)).toEqual([]);
    expect(
      heldRevocations({
        url: "https://a.example",
        platformKeys: [],
        revocations: { content_hash: new Uint8Array(32) },
        updatedAt: 0,
      }),
    ).toEqual([]);
  });
});

describe("revocationsAfter", () => {
  const SERVER = "https://a.example";
  const pinned = [key() as unknown as PlatformKey];
  const r = (n: number): Revocation => ({
    content_hash: new Uint8Array([n]),
    revoked_at: n,
  });
  const offered: RevocationsView = { revocations: [r(2)], signatures: ["ab"] };

  test("holds what the engine accepts, given the pin and what is held", async () => {
    const held = [r(1)];
    const calls: unknown[][] = [];
    const next = await revocationsAfter(
      SERVER,
      pinned,
      held,
      async () => offered,
      (...args) => {
        calls.push(args);
        return [r(1), r(2)];
      },
    );
    expect(next).toEqual([r(1), r(2)]);
    expect(calls).toEqual([[pinned, held, offered]]);
  });

  test("keeps what is held when the list cannot be fetched", async () => {
    const held = [r(1)];
    const next = await revocationsAfter(
      SERVER,
      pinned,
      held,
      async () => {
        throw new Error("offline");
      },
      () => [],
    );
    expect(next).toBe(held);
  });

  test("keeps what is held when no current pinned key signed the list", async () => {
    const held = [r(1)];
    const next = await revocationsAfter(
      SERVER,
      pinned,
      held,
      async () => offered,
      () => {
        throw new Error("no current pinned key signed the revocation list");
      },
    );
    expect(next).toBe(held);
  });

  test("never replaces what is held with a shorter list", async () => {
    const held = [r(1), r(2)];
    const next = await revocationsAfter(
      SERVER,
      pinned,
      held,
      async () => offered,
      () => [r(2)],
    );
    expect(next).toBe(held);
  });
});
