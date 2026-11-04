import { describe, expect, test } from "bun:test";
import type { Session } from "./crisp";
import {
  type DeletionServer,
  deleteAccountFailureMessage,
  deleteAccountFreshly,
  FORBIDDEN,
  logIn,
  REAUTHENTICATION_REQUIRED,
  unpublishFailureMessage,
} from "./serverAccount";

/** A failure as crisp-wasm throws it for a server problem. */
function serverFailure(message: string, code?: string, status = 403): Error {
  const error = new Error(message);
  error.name = "CrispFailure";
  return Object.assign(error, { code, status });
}

/** A fake server that records every call in order. */
function fakeServer(deleteAccount: () => Promise<void> = async () => {}) {
  const calls: string[] = [];
  const server: DeletionServer = {
    challenge: async (username) => {
      calls.push(`challenge:${username}`);
      return { nonce: "n1", audience: "usr1server" };
    },
    login: async (nonce, signature): Promise<Session> => {
      calls.push(`login:${nonce}:${signature.join(",")}`);
      return { user_id: "usr1ada", token: "fresh", expires_at: 99 };
    },
    setToken: (token) => {
      calls.push(`token:${token}`);
    },
    deleteAccount: () => {
      calls.push("deleteAccount");
      return deleteAccount();
    },
  };
  return { server, calls };
}

const signer = {
  signLogin: (audience: string, nonce: string) =>
    new Uint8Array([audience.length, nonce.length]),
};

describe("logIn", () => {
  test("answers a fresh challenge with the key file's signature", async () => {
    const { server, calls } = fakeServer();
    const session = await logIn(server, signer, "ada");
    expect(session.token).toBe("fresh");
    expect(calls).toEqual(["challenge:ada", "login:n1:10,2"]);
  });
});

describe("deleteAccountFreshly", () => {
  test("signs in again and deletes with the new session", async () => {
    const { server, calls } = fakeServer();
    await deleteAccountFreshly(server, signer, "ada");
    expect(calls).toEqual([
      "challenge:ada",
      "login:n1:10,2",
      "token:fresh",
      "deleteAccount",
    ]);
  });

  test("a refused deletion rejects with the server's failure", async () => {
    const refusal = serverFailure("too old", REAUTHENTICATION_REQUIRED);
    const { server } = fakeServer(async () => {
      throw refusal;
    });
    await expect(deleteAccountFreshly(server, signer, "ada")).rejects.toBe(
      refusal,
    );
  });
});

describe("failure messages", () => {
  test("an unpublish of another account's revisions names the publisher", () => {
    expect(unpublishFailureMessage(serverFailure("x", FORBIDDEN))).toContain(
      "another account published",
    );
  });

  test("an unpublish of what is gone says so", () => {
    const missing = serverFailure("no bundle", "not-found", 404);
    expect(unpublishFailureMessage(missing)).toBe(
      "the server no longer has it.",
    );
  });

  test("a stale-session refusal asks for a retry", () => {
    const stale = serverFailure("too old", REAUTHENTICATION_REQUIRED);
    expect(deleteAccountFailureMessage(stale)).toContain("newer sign-in");
  });

  test("anything else is the failure's own message", () => {
    const down = serverFailure("unavailable", "service-unavailable", 503);
    expect(unpublishFailureMessage(down)).toBe("unavailable");
    expect(deleteAccountFailureMessage(new Error("offline"))).toBe("offline");
  });
});
