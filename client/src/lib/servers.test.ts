import { describe, expect, test } from "bun:test";
import {
  endSessionsEverywhere,
  publishFailureMessage,
  QUOTA_EXCEEDED,
  STORAGE_FULL,
} from "./servers";

/** A failure as crisp-wasm throws it for a server problem. */
function serverFailure(message: string, code?: string, status = 403): Error {
  const error = new Error(message);
  error.name = "CrispFailure";
  return Object.assign(error, { code, status });
}

describe("endSessionsEverywhere", () => {
  /** Runs it with `logoutAll` behaving as given; reports whether it cleared. */
  async function run(logoutAll: () => Promise<void>) {
    let cleared = false;
    const done = endSessionsEverywhere(logoutAll, async () => {
      cleared = true;
    });
    return { done, cleared: () => cleared };
  }

  test("clears the local session after the server ends every session", async () => {
    const { done, cleared } = await run(async () => {});
    await done;
    expect(cleared()).toBe(true);
  });

  test("a 401 clears the local session: the server already ended it", async () => {
    const { done, cleared } = await run(async () => {
      throw serverFailure("session expired", "unauthorized", 401);
    });
    await done;
    expect(cleared()).toBe(true);
  });

  test("any other failure keeps the local session and rethrows", async () => {
    for (const failure of [
      serverFailure("unavailable", "unavailable", 503),
      new Error("offline"),
    ]) {
      const { done, cleared } = await run(async () => {
        throw failure;
      });
      await expect(done).rejects.toBe(failure);
      expect(cleared()).toBe(false);
    }
  });
});

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
