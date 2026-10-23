import { describe, expect, test } from "bun:test";
import {
  acknowledgeTimer,
  addTime,
  type Clock,
  createTimerEngine,
  deserializeTimers,
  dismissTimer,
  dueTimers,
  isTimer,
  parseTimers,
  pauseTimer,
  pruneTimers,
  restartTimer,
  resumeTimer,
  serializeTimers,
  settleTimers,
  snapshot,
  startTimer,
  type Timer,
  type TimerInput,
  viewTimer,
} from "./timers";

const MIN = 60_000;
const T0 = 1_700_000_000_000;

/** A manually advanced clock. */
function fakeClock(start = T0): Clock & { advance(ms: number): void } {
  let now = start;
  return {
    now: () => now,
    advance: (ms) => {
      now += ms;
    },
  };
}

function input(id: string, minutes: number, label = id): TimerInput {
  return {
    id,
    label,
    context: { title: "Ragù", step: 3, href: "/recipe/doc1" },
    durationMs: minutes * MIN,
  };
}

function start(
  timers: readonly Timer[],
  id: string,
  minutes: number,
  now = T0,
): readonly Timer[] {
  return startTimer(timers, input(id, minutes), now).timers;
}

describe("startTimer", () => {
  test("stores an absolute end instant, not a tick count", () => {
    const { timer } = startTimer([], input("a", 5), T0);
    expect(timer.state).toBe("running");
    expect(timer.endsAt).toBe(T0 + 5 * MIN);
    expect(timer.remainingMs).toBeNull();
    expect(timer.durationMs).toBe(5 * MIN);
    expect(timer.totalMs).toBe(5 * MIN);
    expect(timer.createdAt).toBe(T0);
    expect(timer.acknowledgedAt).toBeNull();
    expect(timer.context).toEqual({
      title: "Ragù",
      step: 3,
      href: "/recipe/doc1",
    });
  });

  test("generates an id when none is given", () => {
    const { timer } = startTimer(
      [],
      { label: "x", context: { title: "t" }, durationMs: 1000 },
      T0,
    );
    expect(timer.id.length).toBeGreaterThan(0);
  });

  test("rejects non-positive and non-finite durations", () => {
    for (const bad of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(() =>
        startTimer([], { ...input("a", 1), durationMs: bad }, T0),
      ).toThrow(RangeError);
    }
  });

  test("restarts in place for an existing id, keeping createdAt", () => {
    let timers = start([], "a", 5, T0);
    timers = start(timers, "b", 5, T0);
    const { timers: next } = startTimer(timers, input("a", 10), T0 + MIN);
    expect(next).toHaveLength(2);
    const a = next.find((t) => t.id === "a");
    expect(a?.endsAt).toBe(T0 + MIN + 10 * MIN);
    expect(a?.createdAt).toBe(T0);
  });

  test("does not mutate its input", () => {
    const timers = start([], "a", 5);
    const frozen = Object.freeze(timers.slice());
    startTimer(frozen, input("b", 1), T0);
    expect(frozen).toHaveLength(1);
  });
});

describe("pause / resume", () => {
  test("pause captures the remaining time; resume rebases the end", () => {
    let timers = start([], "a", 10);
    timers = pauseTimer(timers, "a", T0 + 3 * MIN);
    expect(timers[0]?.state).toBe("paused");
    expect(timers[0]?.remainingMs).toBe(7 * MIN);
    expect(timers[0]?.endsAt).toBeNull();

    // A long pause does not consume time.
    timers = resumeTimer(timers, "a", T0 + 60 * MIN);
    expect(timers[0]?.state).toBe("running");
    expect(timers[0]?.endsAt).toBe(T0 + 67 * MIN);
    expect(viewTimer(timers[0] as Timer, T0 + 60 * MIN).remainingMs).toBe(
      7 * MIN,
    );
  });

  test("repeated pause/resume cycles accumulate exactly", () => {
    let timers = start([], "a", 10);
    let now = T0;
    for (let i = 0; i < 5; i++) {
      now += 1 * MIN; // run 1 min
      timers = pauseTimer(timers, "a", now);
      now += 17 * MIN + 123; // paused for an arbitrary span
      timers = resumeTimer(timers, "a", now);
    }
    expect(viewTimer(timers[0] as Timer, now).remainingMs).toBe(5 * MIN);
  });

  test("pausing a timer past its end settles it to done instead", () => {
    let timers = start([], "a", 1);
    timers = pauseTimer(timers, "a", T0 + 2 * MIN);
    expect(timers[0]?.state).toBe("done");
    expect(timers[0]?.endsAt).toBe(T0 + MIN);
  });

  test("invalid transitions are no-ops returning the same list", () => {
    const timers = start([], "a", 1);
    expect(resumeTimer(timers, "a", T0)).toBe(timers);
    const paused = pauseTimer(timers, "a", T0);
    expect(pauseTimer(paused, "a", T0)).toBe(paused);
    expect(pauseTimer(timers, "missing", T0)).toBe(timers);
  });
});

describe("addTime", () => {
  test("extends a running timer", () => {
    let timers = start([], "a", 5);
    timers = addTime(timers, "a", MIN, T0 + MIN);
    expect(timers[0]?.endsAt).toBe(T0 + 6 * MIN);
    expect(timers[0]?.totalMs).toBe(6 * MIN);
    expect(timers[0]?.durationMs).toBe(5 * MIN);
  });

  test("extends a paused timer", () => {
    let timers = pauseTimer(start([], "a", 5), "a", T0 + MIN);
    timers = addTime(timers, "a", MIN, T0 + 2 * MIN);
    expect(timers[0]?.remainingMs).toBe(5 * MIN);
    expect(timers[0]?.state).toBe("paused");
  });

  test("snoozes a finished timer into a new run", () => {
    let timers = settleTimers(start([], "a", 1), T0 + 5 * MIN).timers;
    timers = acknowledgeTimer(timers, "a", T0 + 5 * MIN);
    timers = addTime(timers, "a", MIN, T0 + 5 * MIN);
    const t = timers[0] as Timer;
    expect(t.state).toBe("running");
    expect(t.endsAt).toBe(T0 + 6 * MIN);
    expect(t.totalMs).toBe(MIN);
    expect(t.acknowledgedAt).toBeNull();
  });

  test("snoozes a running-but-expired timer from now, not its old end", () => {
    let timers = start([], "a", 1);
    timers = addTime(timers, "a", MIN, T0 + 10 * MIN);
    expect(timers[0]?.endsAt).toBe(T0 + 11 * MIN);
  });

  test("ignores non-positive amounts and dismissed timers", () => {
    const timers = start([], "a", 1);
    expect(addTime(timers, "a", 0, T0)).toBe(timers);
    expect(addTime(timers, "a", -MIN, T0)).toBe(timers);
    const dismissed = dismissTimer(timers, "a");
    expect(addTime(dismissed, "a", MIN, T0)).toBe(dismissed);
  });
});

describe("dismiss / restart / acknowledge", () => {
  test("dismissed timers vanish from the snapshot and are pruned", () => {
    let timers = start(start([], "a", 1), "b", 2);
    timers = dismissTimer(timers, "a");
    expect(snapshot(timers, T0).map((v) => v.id)).toEqual(["b"]);
    expect(pruneTimers(timers).map((t) => t.id)).toEqual(["b"]);
    expect(dismissTimer(timers, "a")).toBe(timers);
  });

  test("pruneTimers returns the same list when nothing is dismissed", () => {
    const timers = start([], "a", 1);
    expect(pruneTimers(timers)).toBe(timers);
  });

  test("restart returns to the configured duration from any state", () => {
    let timers = addTime(start([], "a", 5), "a", 3 * MIN, T0);
    timers = settleTimers(timers, T0 + 20 * MIN).timers;
    timers = restartTimer(timers, "a", T0 + 30 * MIN);
    const t = timers[0] as Timer;
    expect(t.state).toBe("running");
    expect(t.endsAt).toBe(T0 + 35 * MIN);
    expect(t.totalMs).toBe(5 * MIN);
  });

  test("acknowledge only applies to finished timers, once", () => {
    let timers = start([], "a", 1);
    expect(acknowledgeTimer(timers, "a", T0)).toBe(timers);
    timers = acknowledgeTimer(timers, "a", T0 + 2 * MIN);
    expect(timers[0]?.state).toBe("done");
    expect(timers[0]?.acknowledgedAt).toBe(T0 + 2 * MIN);
    expect(acknowledgeTimer(timers, "a", T0 + 3 * MIN)).toBe(timers);
    const view = viewTimer(timers[0] as Timer, T0 + 3 * MIN);
    expect(view.ringing).toBe(false);
    expect(view.overdueMs).toBe(2 * MIN);
  });
});

describe("settle / due", () => {
  test("reports each finished timer exactly once", () => {
    let timers = start(start([], "a", 1), "b", 2);
    expect(dueTimers(timers, T0 + 30_000)).toEqual([]);
    expect(settleTimers(timers, T0 + 30_000).timers).toBe(timers);

    const first = settleTimers(timers, T0 + MIN);
    expect(first.finished.map((t) => t.id)).toEqual(["a"]);
    timers = first.timers;
    expect(settleTimers(timers, T0 + MIN + 1).finished).toEqual([]);

    const second = settleTimers(timers, T0 + 90 * MIN);
    expect(second.finished.map((t) => t.id)).toEqual(["b"]);
    expect(second.finished[0]?.endsAt).toBe(T0 + 2 * MIN);
  });

  test("paused timers never become due", () => {
    const timers = pauseTimer(start([], "a", 1), "a", T0 + 1000);
    expect(dueTimers(timers, T0 + 999 * MIN)).toEqual([]);
  });
});

describe("snapshot", () => {
  test("is drift-free across simulated long gaps", () => {
    const timers = start([], "a", 90);
    // Sparse, irregular "ticks" — as with background throttling or a locked
    // phone — still yield the exact remaining time.
    for (const elapsed of [1, 999, 60_000, 1_234_567, 5_399_999]) {
      const [view] = snapshot(timers, T0 + elapsed);
      expect(view?.remainingMs).toBe(90 * MIN - elapsed);
      expect(view?.state).toBe("running");
    }
    const [late] = snapshot(timers, T0 + 3 * 60 * MIN);
    expect(late?.state).toBe("done");
    expect(late?.remainingMs).toBe(0);
    expect(late?.overdueMs).toBe(90 * MIN);
    expect(late?.ringing).toBe(true);
  });

  test("sorts concurrent timers by remaining, finished first", () => {
    let timers = start([], "long", 30);
    timers = start(timers, "short", 2);
    timers = start(timers, "mid", 10);
    timers = start(timers, "early", 1);
    timers = start(timers, "earliest", 0.5);
    timers = pauseTimer(timers, "mid", T0 + 1000); // ~9 min left, frozen
    const now = T0 + 3 * MIN;
    const views = snapshot(timers, now);
    expect(views.map((v) => v.id)).toEqual([
      "earliest", // overdue 2.5 min
      "early", // overdue 2 min
      "short", // overdue 1 min
      "mid", // paused, 8:59 left
      "long", // 27 min left
    ]);
    expect(views.find((v) => v.id === "mid")?.remainingMs).toBe(
      10 * MIN - 1000,
    );
    expect(views[0]?.overdueMs).toBe(2.5 * MIN);
    expect(views.find((v) => v.id === "short")?.overdueMs).toBe(MIN);
    expect(views.find((v) => v.id === "mid")?.state).toBe("paused");
    expect(views.find((v) => v.id === "long")?.remainingMs).toBe(27 * MIN);
  });

  test("breaks ties by creation time then id", () => {
    let timers = start([], "b", 5, T0);
    timers = start(timers, "a", 5, T0);
    timers = startTimer(timers, input("c", 4), T0 + MIN).timers;
    expect(snapshot(timers, T0).map((v) => v.id)).toEqual(["a", "b", "c"]);
  });

  test("reports progress over the current run", () => {
    const timers = start([], "a", 10);
    expect(snapshot(timers, T0)[0]?.progress).toBe(0);
    expect(snapshot(timers, T0 + 5 * MIN)[0]?.progress).toBe(0.5);
    expect(snapshot(timers, T0 + 50 * MIN)[0]?.progress).toBe(1);
  });
});

describe("serialization", () => {
  test("round-trips through JSON in every state", () => {
    let timers = start([], "run", 5);
    timers = start(timers, "pause", 5);
    timers = start(timers, "done", 1);
    timers = start(timers, "gone", 1);
    timers = pauseTimer(timers, "pause", T0 + 1000);
    timers = settleTimers(timers, T0 + 2 * MIN).timers;
    timers = acknowledgeTimer(timers, "done", T0 + 2 * MIN);
    timers = dismissTimer(timers, "gone");
    const restored = deserializeTimers(serializeTimers(timers));
    expect(restored).toEqual(timers.slice());
    // The views computed after a "reload" match the originals.
    const later = T0 + 4 * MIN;
    expect(snapshot(restored, later)).toEqual(snapshot(timers, later));
  });

  test("survives structured cloning (IndexedDB)", () => {
    const timers = start([], "a", 5);
    expect(parseTimers(structuredClone(timers.slice()))).toEqual(
      timers.slice(),
    );
  });

  test("rejects malformed input", () => {
    expect(deserializeTimers("not json")).toEqual([]);
    expect(deserializeTimers('{"v":2,"timers":[]}')).toEqual([]);
    expect(deserializeTimers('{"v":1,"timers":{}}')).toEqual([]);
    const good = start([], "a", 5)[0] as Timer;
    const bad: unknown[] = [
      null,
      42,
      { ...good, state: "exploded" },
      { ...good, endsAt: null },
      { ...good, state: "paused", remainingMs: null },
      { ...good, durationMs: 0 },
      { ...good, context: { title: 3 } },
      { ...good, context: { title: "t", href: 1 } },
      { ...good, id: 7 },
    ];
    expect(bad.some(isTimer)).toBe(false);
    expect(parseTimers([...bad, good])).toEqual([good]);
  });
});

describe("createTimerEngine", () => {
  test("drives every operation from the injected clock", () => {
    const clock = fakeClock();
    const engine = createTimerEngine(clock);
    let changes = 0;
    const unsubscribe = engine.subscribe(() => {
      changes++;
    });

    engine.start(input("pasta", 9));
    engine.start(input("sauce", 20));
    expect(engine.nextDueInMs()).toBe(9 * MIN);
    clock.advance(4 * MIN);
    engine.pause("sauce");
    clock.advance(10 * MIN);
    expect(engine.due().map((t) => t.id)).toEqual(["pasta"]);
    expect(engine.due()).toEqual([]);
    expect(engine.nextDueInMs()).toBeNull();

    const views = engine.snapshot();
    expect(views.map((v) => [v.id, v.state])).toEqual([
      ["pasta", "done"],
      ["sauce", "paused"],
    ]);
    expect(views[0]?.overdueMs).toBe(5 * MIN);
    expect(views[1]?.remainingMs).toBe(16 * MIN);

    engine.acknowledge("pasta");
    engine.dismiss("pasta");
    engine.resume("sauce");
    engine.addTime("sauce", MIN);
    clock.advance(17 * MIN);
    expect(engine.due().map((t) => t.id)).toEqual(["sauce"]);
    engine.restart("sauce");
    expect(engine.snapshot()[0]?.remainingMs).toBe(20 * MIN);
    expect(engine.getTimers()).toHaveLength(1);

    const before = changes;
    engine.pause("missing");
    expect(changes).toBe(before);
    unsubscribe();
    engine.pause("sauce");
    expect(changes).toBe(before);
  });

  test("load replaces state (e.g. after reload) and keeps exact time", () => {
    const clock = fakeClock();
    const first = createTimerEngine(clock);
    first.start(input("a", 5));
    const saved = serializeTimers(first.getTimers());

    clock.advance(3 * MIN + 1234); // app closed / phone locked
    const second = createTimerEngine(clock);
    second.load(deserializeTimers(saved));
    expect(second.snapshot()[0]?.remainingMs).toBe(2 * MIN - 1234);
    clock.advance(10 * MIN);
    expect(second.due().map((t) => t.id)).toEqual(["a"]);
  });
});
