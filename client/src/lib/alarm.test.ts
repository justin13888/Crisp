import { describe, expect, test } from "bun:test";
import {
  type AlarmDeps,
  BEEP,
  CYCLE_MS,
  createAlarm,
  type NotifyPermission,
  notificationTag,
  type ToneOutput,
  VIBRATION,
} from "./alarm";

interface Fakes {
  deps: AlarmDeps;
  contexts: number;
  tones: number;
  resumes: number;
  vibrations: (number | number[])[];
  shown: string[];
  closed: string[];
  permissionRequests: number;
  intervals: { fn: () => void; ms: number; cancelled: boolean }[];
  permission: NotifyPermission;
}

function fakes(
  overrides: Partial<AlarmDeps> = {},
  options: { state?: string; permission?: NotifyPermission } = {},
): Fakes {
  const f: Fakes = {
    contexts: 0,
    tones: 0,
    resumes: 0,
    vibrations: [],
    shown: [],
    closed: [],
    permissionRequests: 0,
    intervals: [],
    permission: options.permission ?? "default",
    deps: undefined as unknown as AlarmDeps,
  };
  const ctx: ToneOutput = {
    state: options.state ?? "running",
    resume: async () => {
      f.resumes++;
    },
    now: () => 0,
    tone: () => {
      f.tones++;
    },
  };
  f.deps = {
    createAudio: () => {
      f.contexts++;
      return ctx;
    },
    vibrate: (pattern) => {
      f.vibrations.push(pattern);
      return true;
    },
    notifier: {
      permission: () => f.permission,
      requestPermission: async () => {
        f.permissionRequests++;
        f.permission = "granted";
        return f.permission;
      },
      show: async (tag) => {
        f.shown.push(tag);
      },
      close: async (tag) => {
        f.closed.push(tag);
      },
    },
    every: (fn, ms) => {
      const entry = { fn, ms, cancelled: false };
      f.intervals.push(entry);
      return () => {
        entry.cancelled = true;
      };
    },
    ...overrides,
  };
  return f;
}

const info = (id: string) => ({ id, title: `${id} done`, body: "Stew" });

describe("createAlarm", () => {
  test("prime creates audio lazily and asks for notification permission", async () => {
    const f = fakes();
    const alarm = createAlarm(f.deps);
    expect(f.contexts).toBe(0);
    alarm.prime();
    alarm.prime();
    await Promise.resolve();
    expect(f.contexts).toBe(1);
    expect(f.permissionRequests).toBe(1);
  });

  test("prime resumes a suspended context", () => {
    const f = fakes({}, { state: "suspended" });
    createAlarm(f.deps).prime();
    expect(f.resumes).toBe(1);
  });

  test("fire beeps, vibrates, notifies and repeats until stopped", () => {
    const f = fakes({}, { permission: "granted" });
    const alarm = createAlarm(f.deps);
    alarm.fire(info("a"));
    expect(f.tones).toBe(BEEP.count);
    expect(f.vibrations).toEqual([VIBRATION]);
    expect(f.shown).toEqual([notificationTag("a")]);
    expect(f.intervals).toHaveLength(1);
    expect(f.intervals[0]?.ms).toBe(CYCLE_MS);

    f.intervals[0]?.fn();
    f.intervals[0]?.fn();
    expect(f.tones).toBe(3 * BEEP.count);

    alarm.stop("a");
    expect(f.intervals[0]?.cancelled).toBe(true);
    expect(f.vibrations.at(-1)).toBe(0);
    expect(f.closed).toEqual([notificationTag("a")]);
    expect(alarm.ringing()).toEqual([]);
  });

  test("multiple timers share one beep loop; it stops with the last", () => {
    const f = fakes({}, { permission: "granted" });
    const alarm = createAlarm(f.deps);
    alarm.fire(info("a"));
    alarm.fire(info("b"));
    alarm.fire(info("a")); // idempotent
    expect(f.intervals).toHaveLength(1);
    expect(f.shown).toEqual([notificationTag("a"), notificationTag("b")]);
    alarm.stop("a");
    expect(f.intervals[0]?.cancelled).toBe(false);
    expect(alarm.ringing()).toEqual(["b"]);
    alarm.stop();
    expect(f.intervals[0]?.cancelled).toBe(true);
    expect(alarm.ringing()).toEqual([]);
    alarm.stop("unknown");
  });

  test("does not notify without permission", () => {
    const f = fakes({}, { permission: "denied" });
    createAlarm(f.deps).fire(info("a"));
    expect(f.shown).toEqual([]);
  });

  test("works with every capability missing", () => {
    const f = fakes({
      createAudio: null,
      vibrate: null,
      notifier: null,
    });
    const alarm = createAlarm(f.deps);
    alarm.prime();
    alarm.fire(info("a"));
    expect(alarm.ringing()).toEqual(["a"]);
    alarm.stop();
    expect(alarm.ringing()).toEqual([]);
  });

  test("tolerates throwing capabilities", async () => {
    const f = fakes(
      {
        createAudio: () => {
          throw new Error("no audio");
        },
        vibrate: () => {
          throw new Error("no vibrate");
        },
      },
      { permission: "granted" },
    );
    f.deps.notifier = {
      permission: () => "granted",
      requestPermission: async () => "granted",
      show: async () => {
        throw new Error("blocked");
      },
      close: async () => {
        throw new Error("gone");
      },
    };
    const alarm = createAlarm(f.deps);
    expect(() => alarm.fire(info("a"))).not.toThrow();
    expect(() => alarm.stop()).not.toThrow();
    await Promise.resolve();
  });

  test("the beep cycle fits inside the repeat interval", () => {
    expect(BEEP.count * (BEEP.onMs + BEEP.offMs)).toBeLessThan(CYCLE_MS);
    const vibrationMs = VIBRATION.reduce((a, b) => a + b, 0);
    expect(vibrationMs).toBeLessThan(CYCLE_MS);
  });
});
