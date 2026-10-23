import { describe, expect, test } from "bun:test";
import {
  isEditableTarget,
  type KeyEventLike,
  matchesHotkey,
  parseHotkey,
} from "./useHotkeys";
import { nextTickDelay } from "./useNow";
import { classifySwipe } from "./useSwipe";
import { createWakeLockController, type SentinelLike } from "./useWakeLock";

describe("nextTickDelay", () => {
  test("aligns ticks to interval boundaries", () => {
    expect(nextTickDelay(10_000, 1000)).toBe(1000);
    expect(nextTickDelay(10_001, 1000)).toBe(999);
    expect(nextTickDelay(10_999, 1000)).toBe(1);
    expect(nextTickDelay(1_234, 250)).toBe(16);
  });
});

describe("classifySwipe", () => {
  test("recognizes horizontal swipes", () => {
    expect(classifySwipe({ dx: -120, dy: 10, dt: 200 })).toBe("left");
    expect(classifySwipe({ dx: 90, dy: -20, dt: 300 })).toBe("right");
  });

  test("rejects short, slow, and vertical gestures", () => {
    expect(classifySwipe({ dx: -30, dy: 0, dt: 100 })).toBeNull();
    expect(classifySwipe({ dx: -200, dy: 0, dt: 2000 })).toBeNull();
    expect(classifySwipe({ dx: 100, dy: 90, dt: 200 })).toBeNull();
    expect(classifySwipe({ dx: 100, dy: 0, dt: -1 })).toBeNull();
  });

  test("honours custom thresholds", () => {
    expect(classifySwipe({ dx: 30, dy: 0, dt: 100 }, { minDistance: 20 })).toBe(
      "right",
    );
    expect(classifySwipe({ dx: 100, dy: 90, dt: 200 }, { maxSlope: 1 })).toBe(
      "right",
    );
  });
});

function key(
  k: string,
  mods: Partial<Omit<KeyEventLike, "key">> = {},
): KeyEventLike {
  return {
    key: k,
    shiftKey: false,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    ...mods,
  };
}

describe("hotkeys", () => {
  test("parses modifiers and aliases", () => {
    expect(parseHotkey("ArrowRight")).toEqual({
      key: "arrowright",
      shift: false,
      alt: false,
      ctrl: false,
      meta: false,
    });
    expect(parseHotkey("space").key).toBe(" ");
    expect(parseHotkey("shift+N")).toMatchObject({ key: "n", shift: true });
    expect(parseHotkey("mod+k")).toMatchObject({ ctrl: true, meta: false });
    expect(parseHotkey("mod+k", true)).toMatchObject({
      ctrl: false,
      meta: true,
    });
  });

  test("matches exactly, case-insensitively", () => {
    expect(matchesHotkey(key("ArrowRight"), parseHotkey("right"))).toBe(true);
    expect(
      matchesHotkey(key("N", { shiftKey: true }), parseHotkey("shift+n")),
    ).toBe(true);
    expect(matchesHotkey(key("n", { ctrlKey: true }), parseHotkey("n"))).toBe(
      false,
    );
    expect(matchesHotkey(key(" "), parseHotkey("space"))).toBe(true);
    expect(matchesHotkey(key("j"), parseHotkey("k"))).toBe(false);
  });

  test("non-elements are not editable targets", () => {
    expect(isEditableTarget(null)).toBe(false);
  });
});

/** A controllable fake sentinel. */
function fakeSentinel(): SentinelLike & { drop(): void } {
  const listeners: (() => void)[] = [];
  const sentinel = {
    released: false,
    release: async () => {
      sentinel.released = true;
      for (const l of listeners) l();
    },
    addEventListener: (_type: "release", listener: () => void) => {
      listeners.push(listener);
    },
    drop: () => {
      sentinel.released = true;
      for (const l of listeners) l();
    },
  };
  return sentinel;
}

describe("createWakeLockController", () => {
  test("acquires when wanted and releases when not", async () => {
    const sentinels: ReturnType<typeof fakeSentinel>[] = [];
    const changes: boolean[] = [];
    const c = createWakeLockController(
      {
        request: async () => {
          const s = fakeSentinel();
          sentinels.push(s);
          return s;
        },
        isVisible: () => true,
      },
      (active) => changes.push(active),
    );
    expect(c.supported).toBe(true);
    await c.setWanted(true);
    expect(c.isActive()).toBe(true);
    await c.setWanted(true); // no duplicate request
    expect(sentinels).toHaveLength(1);
    await c.setWanted(false);
    expect(c.isActive()).toBe(false);
    expect(sentinels[0]?.released).toBe(true);
    expect(changes[0]).toBe(true);
    expect(changes.at(-1)).toBe(false);
  });

  test("re-acquires after the browser drops the lock on hide", async () => {
    let visible = true;
    const sentinels: ReturnType<typeof fakeSentinel>[] = [];
    const c = createWakeLockController({
      request: async () => {
        const s = fakeSentinel();
        sentinels.push(s);
        return s;
      },
      isVisible: () => visible,
    });
    await c.setWanted(true);
    visible = false;
    sentinels[0]?.drop();
    expect(c.isActive()).toBe(false);
    await c.onVisibilityChange(); // still hidden: nothing
    expect(sentinels).toHaveLength(1);
    visible = true;
    await c.onVisibilityChange();
    expect(sentinels).toHaveLength(2);
    expect(c.isActive()).toBe(true);
  });

  test("does not re-acquire when no longer wanted", async () => {
    let requests = 0;
    const c = createWakeLockController({
      request: async () => {
        requests++;
        return fakeSentinel();
      },
      isVisible: () => true,
    });
    await c.onVisibilityChange();
    expect(requests).toBe(0);
  });

  test("is inert and failure-tolerant without support", async () => {
    const unsupported = createWakeLockController({
      request: null,
      isVisible: () => true,
    });
    expect(unsupported.supported).toBe(false);
    await unsupported.setWanted(true);
    expect(unsupported.isActive()).toBe(false);

    const failing = createWakeLockController({
      request: async () => {
        throw new Error("NotAllowedError");
      },
      isVisible: () => true,
    });
    await failing.setWanted(true);
    expect(failing.isActive()).toBe(false);
  });
});
