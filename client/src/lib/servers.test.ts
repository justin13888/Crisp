import { describe, expect, test } from "bun:test";
import { isPlatformKey } from "./servers";

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
});
