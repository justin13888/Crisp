import { describe, expect, test } from "bun:test";
import {
  type CacheStorageLike,
  type IncomingFile,
  onLaunchedFiles,
  SHARE_INBOX,
  takeSharedFiles,
} from "./incomingFiles";

function fakeCaches(entries: [string, Response][]): {
  api: CacheStorageLike;
  opened: string[];
  remaining: () => number;
} {
  const store = new Map(entries);
  const opened: string[] = [];
  return {
    opened,
    remaining: () => store.size,
    api: {
      open: async (name) => {
        opened.push(name);
        return {
          keys: async () =>
            [...store.keys()].map((url) => new Request(`https://x${url}`)),
          match: async (req) => store.get(new URL(req.url).pathname),
          delete: async (req) => store.delete(new URL(req.url).pathname),
        };
      },
    },
  };
}

describe("takeSharedFiles", () => {
  test("returns and clears the share inbox", async () => {
    const caches = fakeCaches([
      [
        "/share-inbox/1-0",
        new Response(new Uint8Array([1, 2]), {
          headers: {
            "content-type": "application/crisp+cbor",
            "x-filename": encodeURIComponent("Grandma's soup.crisp"),
          },
        }),
      ],
    ]);
    const files = await takeSharedFiles(caches.api);
    expect(caches.opened).toEqual([SHARE_INBOX]);
    expect(files).toEqual([
      {
        name: "Grandma's soup.crisp",
        type: "application/crisp+cbor",
        bytes: new Uint8Array([1, 2]),
      },
    ]);
    expect(caches.remaining()).toBe(0);
    expect(await takeSharedFiles(caches.api)).toEqual([]);
  });

  test("never rejects", async () => {
    expect(await takeSharedFiles(undefined)).toEqual([]);
    expect(
      await takeSharedFiles({
        open: async () => {
          throw new Error("SecurityError");
        },
      }),
    ).toEqual([]);
  });
});

describe("onLaunchedFiles", () => {
  test("reads files from launch params", async () => {
    let consumer: ((params: unknown) => Promise<void>) | null = null;
    const queue = {
      setConsumer: (fn: (params: unknown) => Promise<void>) => {
        consumer = fn;
      },
    };
    const received: IncomingFile[][] = [];
    expect(onLaunchedFiles((files) => received.push(files), queue)).toBe(true);
    const run = consumer as ((params: unknown) => Promise<void>) | null;
    await run?.({
      files: [
        { getFile: async () => new File([new Uint8Array([7])], "a.crisp") },
        { notAHandle: true },
        {
          getFile: async () => {
            throw new Error("NotAllowedError");
          },
        },
      ],
    });
    expect(received).toHaveLength(1);
    expect(received[0]?.[0]?.name).toBe("a.crisp");
    expect(received[0]?.[0]?.bytes).toEqual(new Uint8Array([7]));
    await run?.({ files: [] });
    await run?.(null);
    expect(received).toHaveLength(1);
  });

  test("reports lack of support", () => {
    expect(onLaunchedFiles(() => undefined, undefined)).toBe(false);
    expect(onLaunchedFiles(() => undefined, {})).toBe(false);
  });
});
