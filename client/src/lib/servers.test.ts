import { describe, expect, test } from "bun:test";
import {
  isPlatformKey,
  publishFailureMessage,
  QUOTA_EXCEEDED,
  STORAGE_FULL,
} from "./servers";

/** A failure as crisp-wasm throws it for a server problem. */
function serverFailure(message: string, code?: string): Error {
  const error = new Error(message);
  error.name = "CrispFailure";
  return Object.assign(error, { code, status: 403 });
}

describe("publishFailureMessage", () => {
  test("a quota refusal points at deleting old revisions or documents", () => {
    const message = publishFailureMessage(
      serverFailure("publishing this would exceed…", QUOTA_EXCEEDED),
    );
    expect(message).toContain("Delete old revisions or documents");
  });

  test("a full server says to try later", () => {
    const message = publishFailureMessage(
      serverFailure("the server has no room", STORAGE_FULL),
    );
    expect(message).toContain("Try again later");
  });

  test("any other failure keeps the server's message", () => {
    expect(publishFailureMessage(serverFailure("bundle failed", "bad"))).toBe(
      "bundle failed",
    );
    expect(publishFailureMessage(new Error("offline"))).toBe("offline");
    expect(publishFailureMessage("boom")).toBe("boom");
  });
});

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
