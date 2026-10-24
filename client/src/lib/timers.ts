/**
 * The cooking timer engine — pure, clock-injected, drift-free.
 *
 * A timer never counts ticks. A running timer stores the absolute epoch-ms
 * instant it ends (`endsAt`); a paused one stores the milliseconds it has
 * left (`remainingMs`). Remaining time is always derived from `now`, so
 * timers stay exact across reloads, background-tab throttling, and a locked
 * phone: however late the next tick arrives, the arithmetic is the same.
 *
 * Two layers:
 *  - pure functions over an immutable `readonly Timer[]` that take `now`
 *    explicitly (reducer-friendly and trivially testable), and
 *  - {@link createTimerEngine}, a small stateful wrapper with an injected
 *    {@link Clock} and change subscription (for `useSyncExternalStore`).
 */

import { isRecord } from "./isRecord";
import { createLogger } from "./log";

const log = createLogger("timers");

/** A source of the current time in epoch milliseconds. */
export interface Clock {
  /** The current instant (epoch ms). */
  now(): number;
}

/** The wall clock. */
export const systemClock: Clock = { now: () => Date.now() };

/** A timer's lifecycle state. */
export type TimerState = "running" | "paused" | "done" | "dismissed";

/** Where a timer came from, so the UI can label it and navigate back. */
export interface TimerContext {
  /** E.g. the recipe title. */
  title: string;
  /** The 1-based step number the timer belongs to, if any. */
  step?: number;
  /** An opaque in-app URL that returns to the timer's origin. */
  href?: string;
}

/** A persisted timer. Plain data — safe to structured-clone or JSON. */
export interface Timer {
  /** Stable identity. Starting a timer with an existing id restarts it. */
  id: string;
  /** Short label, e.g. "Simmer sauce". */
  label: string;
  /** Origin of the timer. */
  context: TimerContext;
  /** The configured duration; {@link restartTimer} returns to it. */
  durationMs: number;
  /** The span of the current run including added time (for progress). */
  totalMs: number;
  /** Lifecycle state. */
  state: TimerState;
  /** `running`: when it will end. `done`: when it ended. Otherwise `null`. */
  endsAt: number | null;
  /** `paused`: milliseconds left. Otherwise `null`. */
  remainingMs: number | null;
  /** When a finished timer's alarm was acknowledged, else `null`. */
  acknowledgedAt: number | null;
  /** When the timer was first started (epoch ms). */
  createdAt: number;
}

/** What a caller supplies to start a timer. */
export interface TimerInput {
  /** Optional stable id (e.g. `${docId}#step-3`); generated when omitted. */
  id?: string;
  /** Short label. */
  label: string;
  /** Origin of the timer. */
  context: TimerContext;
  /** Duration in milliseconds; must be a positive finite number. */
  durationMs: number;
}

/** A derived, display-ready view of one timer at an instant. */
export interface TimerView {
  /** The underlying record. */
  timer: Timer;
  /** The timer id. */
  id: string;
  /** The timer label. */
  label: string;
  /** The timer context. */
  context: TimerContext;
  /** The effective state: a running timer past its end reads as `done`. */
  state: Exclude<TimerState, "dismissed">;
  /** Milliseconds left (never negative). */
  remainingMs: number;
  /** Milliseconds since it finished (0 unless done). */
  overdueMs: number;
  /** Elapsed fraction of the current run, 0…1. */
  progress: number;
  /** Finished and not yet acknowledged — the alarm should sound. */
  ringing: boolean;
}

/** The result of settling timers at an instant. */
export interface SettleResult {
  /** The updated list (same reference when nothing finished). */
  timers: readonly Timer[];
  /** Timers that transitioned to `done` in this call. */
  finished: Timer[];
}

/** Whether `ms` is a usable positive duration. */
function isPositiveDuration(ms: number): boolean {
  return Number.isFinite(ms) && ms > 0;
}

/** Whether a running timer has reached its end at `now`. */
function hasEnded(timer: Timer, now: number): boolean {
  return (
    timer.state === "running" && timer.endsAt !== null && timer.endsAt <= now
  );
}

/** Settles one timer at `now` (running → done when its end has passed). */
function settleOne(timer: Timer, now: number): Timer {
  return hasEnded(timer, now) ? { ...timer, state: "done" } : timer;
}

/** Applies `fn` to the timer with `id`; returns the same list if unchanged. */
function updateTimer(
  timers: readonly Timer[],
  id: string,
  op: string,
  fn: (timer: Timer) => Timer,
): readonly Timer[] {
  const index = timers.findIndex((t) => t.id === id);
  const current = timers[index];
  if (current === undefined) {
    log.debug("timer.unknown", { op, id });
    return timers;
  }
  const next = fn(current);
  if (next === current) {
    log.debug("timer.noop", { op, id, state: current.state });
    return timers;
  }
  log.info(`timer.${op}`, {
    id,
    from: current.state,
    to: next.state,
    endsAt: next.endsAt,
    remainingMs: next.remainingMs,
  });
  const copy = timers.slice();
  copy[index] = next;
  return copy;
}

/** Generates a timer id. */
export function newTimerId(): string {
  return (
    globalThis.crypto?.randomUUID?.() ?? `t-${Date.now()}-${Math.random()}`
  );
}

/**
 * Starts a timer. If a timer with the same id exists it is replaced by a
 * fresh run (keeping its `createdAt`). Throws `RangeError` on a non-positive
 * or non-finite duration.
 */
export function startTimer(
  timers: readonly Timer[],
  input: TimerInput,
  now: number,
): { timers: readonly Timer[]; timer: Timer } {
  if (!isPositiveDuration(input.durationMs)) {
    throw new RangeError(`invalid timer duration: ${input.durationMs}`);
  }
  const id = input.id ?? newTimerId();
  const existing = timers.find((t) => t.id === id);
  const timer: Timer = {
    id,
    label: input.label,
    context: { ...input.context },
    durationMs: input.durationMs,
    totalMs: input.durationMs,
    state: "running",
    endsAt: now + input.durationMs,
    remainingMs: null,
    acknowledgedAt: null,
    createdAt: existing?.createdAt ?? now,
  };
  log.info("timer.start", {
    id,
    label: timer.label,
    durationMs: timer.durationMs,
    endsAt: timer.endsAt,
    replaced: existing !== undefined,
  });
  const next = existing
    ? timers.map((t) => (t.id === id ? timer : t))
    : [...timers, timer];
  return { timers: next, timer };
}

/** Pauses a running timer. A timer already past its end becomes `done`. */
export function pauseTimer(
  timers: readonly Timer[],
  id: string,
  now: number,
): readonly Timer[] {
  return updateTimer(timers, id, "pause", (t) => {
    if (t.state !== "running" || t.endsAt === null) return t;
    if (hasEnded(t, now)) return settleOne(t, now);
    return {
      ...t,
      state: "paused",
      remainingMs: t.endsAt - now,
      endsAt: null,
    };
  });
}

/** Resumes a paused timer from where it stopped. */
export function resumeTimer(
  timers: readonly Timer[],
  id: string,
  now: number,
): readonly Timer[] {
  return updateTimer(timers, id, "resume", (t) => {
    if (t.state !== "paused" || t.remainingMs === null) return t;
    return {
      ...t,
      state: "running",
      endsAt: now + t.remainingMs,
      remainingMs: null,
    };
  });
}

/**
 * Adds time. Running/paused timers are extended; a finished timer starts a
 * new run of `ms` ("+1 min" snooze). Non-positive `ms` is a no-op.
 */
export function addTime(
  timers: readonly Timer[],
  id: string,
  ms: number,
  now: number,
): readonly Timer[] {
  if (!isPositiveDuration(ms)) return timers;
  return updateTimer(timers, id, "add-time", (raw) => {
    const t = settleOne(raw, now);
    switch (t.state) {
      case "running":
        return {
          ...t,
          endsAt: (t.endsAt ?? now) + ms,
          totalMs: t.totalMs + ms,
        };
      case "paused":
        return {
          ...t,
          remainingMs: (t.remainingMs ?? 0) + ms,
          totalMs: t.totalMs + ms,
        };
      case "done":
        return {
          ...t,
          state: "running",
          endsAt: now + ms,
          totalMs: ms,
          remainingMs: null,
          acknowledgedAt: null,
        };
      case "dismissed":
        return raw;
    }
  });
}

/** Dismisses a timer (hidden from views; dropped by {@link pruneTimers}). */
export function dismissTimer(
  timers: readonly Timer[],
  id: string,
): readonly Timer[] {
  return updateTimer(timers, id, "dismiss", (t) =>
    t.state === "dismissed" ? t : { ...t, state: "dismissed" },
  );
}

/** Restarts a timer from its configured duration. */
export function restartTimer(
  timers: readonly Timer[],
  id: string,
  now: number,
): readonly Timer[] {
  return updateTimer(timers, id, "restart", (t) => ({
    ...t,
    state: "running",
    endsAt: now + t.durationMs,
    totalMs: t.durationMs,
    remainingMs: null,
    acknowledgedAt: null,
  }));
}

/** Acknowledges a finished timer's alarm (it keeps showing as done). */
export function acknowledgeTimer(
  timers: readonly Timer[],
  id: string,
  now: number,
): readonly Timer[] {
  return updateTimer(timers, id, "acknowledge", (raw) => {
    const t = settleOne(raw, now);
    if (t.state !== "done" || t.acknowledgedAt !== null) return raw;
    return { ...t, acknowledgedAt: now };
  });
}

/**
 * Transitions every running timer past its end to `done`, reporting each
 * exactly once — the caller fires the alarm for `finished`.
 */
export function settleTimers(
  timers: readonly Timer[],
  now: number,
): SettleResult {
  const finished: Timer[] = [];
  const next = timers.map((t) => {
    const settled = settleOne(t, now);
    if (settled !== t) finished.push(settled);
    return settled;
  });
  if (finished.length === 0) return { timers, finished };
  for (const t of finished) {
    log.info("timer.finished", {
      id: t.id,
      label: t.label,
      endsAt: t.endsAt,
      lateMs: now - (t.endsAt ?? now),
    });
  }
  return { timers: next, finished };
}

/** Drops dismissed timers. */
export function pruneTimers(timers: readonly Timer[]): readonly Timer[] {
  return timers.some((t) => t.state === "dismissed")
    ? timers.filter((t) => t.state !== "dismissed")
    : timers;
}

/** Signed milliseconds to the end: negative once finished. */
function signedRemaining(t: Timer, now: number): number {
  if (t.state === "paused") return t.remainingMs ?? 0;
  return (t.endsAt ?? now) - now;
}

/** Builds the display view of one non-dismissed timer at `now`. */
export function viewTimer(timer: Timer, now: number): TimerView {
  const t = settleOne(timer, now);
  const state = t.state === "dismissed" ? "done" : t.state;
  const signed = signedRemaining(t, now);
  const remainingMs = state === "done" ? 0 : Math.max(0, signed);
  const overdueMs = state === "done" ? Math.max(0, -signed) : 0;
  const progress =
    t.totalMs > 0 ? Math.min(1, Math.max(0, 1 - remainingMs / t.totalMs)) : 1;
  return {
    timer,
    id: t.id,
    label: t.label,
    context: t.context,
    state,
    remainingMs,
    overdueMs,
    progress,
    ringing: state === "done" && t.acknowledgedAt === null,
  };
}

/**
 * The visible timers at `now`, most urgent first: finished timers (longest
 * overdue first), then by time remaining. Ties break on `createdAt`, then id.
 */
export function snapshot(timers: readonly Timer[], now: number): TimerView[] {
  return timers
    .filter((t) => t.state !== "dismissed")
    .map((t) => ({ t, key: signedRemaining(settleOne(t, now), now) }))
    .sort(
      (a, b) =>
        a.key - b.key ||
        a.t.createdAt - b.t.createdAt ||
        (a.t.id < b.t.id ? -1 : a.t.id > b.t.id ? 1 : 0),
    )
    .map(({ t }) => viewTimer(t, now));
}

function isNullableNumber(value: unknown): value is number | null {
  return (
    value === null || (typeof value === "number" && Number.isFinite(value))
  );
}

const STATES: readonly string[] = ["running", "paused", "done", "dismissed"];

/** Guards a persisted value (IndexedDB / JSON) as a well-formed timer. */
export function isTimer(value: unknown): value is Timer {
  if (!isRecord(value)) return false;
  const ctx = value["context"];
  if (
    !isRecord(ctx) ||
    typeof ctx["title"] !== "string" ||
    (ctx["step"] !== undefined && typeof ctx["step"] !== "number") ||
    (ctx["href"] !== undefined && typeof ctx["href"] !== "string")
  ) {
    return false;
  }
  const state = value["state"];
  if (typeof state !== "string" || !STATES.includes(state)) return false;
  const endsAt = value["endsAt"];
  const remainingMs = value["remainingMs"];
  if (!isNullableNumber(endsAt) || !isNullableNumber(remainingMs)) return false;
  if ((state === "running" || state === "done") && endsAt === null) {
    return false;
  }
  if (state === "paused" && remainingMs === null) return false;
  return (
    typeof value["id"] === "string" &&
    typeof value["label"] === "string" &&
    typeof value["durationMs"] === "number" &&
    isPositiveDuration(value["durationMs"]) &&
    typeof value["totalMs"] === "number" &&
    Number.isFinite(value["totalMs"]) &&
    isNullableNumber(value["acknowledgedAt"]) &&
    typeof value["createdAt"] === "number" &&
    Number.isFinite(value["createdAt"])
  );
}

/** Keeps only well-formed timers from untrusted persisted values. */
export function parseTimers(values: readonly unknown[]): Timer[] {
  const timers = values.filter(isTimer);
  if (timers.length !== values.length) {
    log.warn("timer.parse.dropped", {
      dropped: values.length - timers.length,
    });
  }
  return timers;
}

/** A stateful engine over the pure functions, with an injected clock. */
export interface TimerEngine {
  /** The current timers (immutable; replaced on every change). */
  getTimers(): readonly Timer[];
  /** Replaces all timers (e.g. after loading from storage). */
  load(timers: readonly Timer[]): void;
  /** Starts (or restarts, for a known id) a timer. */
  start(input: TimerInput): Timer;
  /** Pauses a running timer. */
  pause(id: string): void;
  /** Resumes a paused timer. */
  resume(id: string): void;
  /** Adds time to a timer (snoozes a finished one). */
  addTime(id: string, ms: number): void;
  /** Dismisses and drops a timer. */
  dismiss(id: string): void;
  /** Restarts a timer from its configured duration. */
  restart(id: string): void;
  /** Acknowledges a finished timer's alarm. */
  acknowledge(id: string): void;
  /** The display view at the clock's now. */
  snapshot(): TimerView[];
  /** Settles timers and returns those that just finished. */
  due(): Timer[];
  /** Milliseconds until the next running timer ends, or `null`. */
  nextDueInMs(): number | null;
  /** Subscribes to changes; returns an unsubscribe function. */
  subscribe(listener: () => void): () => void;
}

/** Creates a {@link TimerEngine}. */
export function createTimerEngine(
  clock: Clock = systemClock,
  initial: readonly Timer[] = [],
): TimerEngine {
  let timers: readonly Timer[] = initial;
  const listeners = new Set<() => void>();
  const commit = (next: readonly Timer[]): void => {
    if (next === timers) return;
    timers = next;
    for (const listener of listeners) listener();
  };
  return {
    getTimers: () => timers,
    load: (next) => {
      log.debug("engine.load", { count: next.length });
      commit(next.slice());
    },
    start: (input) => {
      const result = startTimer(timers, input, clock.now());
      commit(result.timers);
      return result.timer;
    },
    pause: (id) => commit(pauseTimer(timers, id, clock.now())),
    resume: (id) => commit(resumeTimer(timers, id, clock.now())),
    addTime: (id, ms) => commit(addTime(timers, id, ms, clock.now())),
    dismiss: (id) => commit(pruneTimers(dismissTimer(timers, id))),
    restart: (id) => commit(restartTimer(timers, id, clock.now())),
    acknowledge: (id) => commit(acknowledgeTimer(timers, id, clock.now())),
    snapshot: () => snapshot(timers, clock.now()),
    due: () => {
      const result = settleTimers(timers, clock.now());
      commit(result.timers);
      return result.finished;
    },
    nextDueInMs: () => {
      const now = clock.now();
      let next: number | null = null;
      for (const t of timers) {
        if (t.state !== "running" || t.endsAt === null) continue;
        const delta = Math.max(0, t.endsAt - now);
        if (next === null || delta < next) next = delta;
      }
      return next;
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
