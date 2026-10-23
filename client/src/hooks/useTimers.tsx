import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { type Alarm, createAlarm } from "../lib/alarm";
import { createLogger } from "../lib/log";
import { getStore, requestPersistence } from "../lib/store";
import {
  type Clock,
  createTimerEngine,
  snapshot,
  systemClock,
  type Timer,
  type TimerInput,
  type TimerView,
} from "../lib/timers";
import { useNow } from "./useNow";

const log = createLogger("timers");

/** Where timers are saved between page loads. */
export interface TimerPersistence {
  /** Loads saved timers (already validated). */
  load(): Promise<Timer[]>;
  /** Replaces the saved set. */
  save(timers: readonly Timer[]): Promise<void>;
}

/** Persists timers in the IndexedDB `timers` store. */
export const storeTimerPersistence: TimerPersistence = {
  load: async () => (await getStore()).timers.all(),
  save: async (timers) => (await getStore()).timers.replaceAll(timers),
};

/** The timers API exposed to components. */
export interface TimersApi {
  /** Visible timers, most urgent first, at {@link TimersApi.now}. */
  views: TimerView[];
  /** The instant the views were computed for. */
  now: number;
  /** The latest screen-reader announcement (e.g. "Pasta timer is done"). */
  announcement: string;
  /** Whether saved timers have been loaded. */
  ready: boolean;
  /** Starts a timer (call from a user gesture: it also primes the alarm). */
  start(input: TimerInput): Timer;
  /** Pauses a running timer. */
  pause(id: string): void;
  /** Resumes a paused timer. */
  resume(id: string): void;
  /** Adds time; snoozes a finished timer. */
  addTime(id: string, ms: number): void;
  /** Removes a timer. */
  dismiss(id: string): void;
  /** Restarts a timer from its configured duration. */
  restart(id: string): void;
  /** Silences a finished timer's alarm. */
  acknowledge(id: string): void;
}

const TimersContext = createContext<TimersApi | null>(null);

/** Provider configuration (all injectable for tests). */
export interface TimersProviderProps {
  /** The subtree. */
  children: ReactNode;
  /** Time source (default: wall clock). */
  clock?: Clock;
  /** Alarm (default: browser alarm). */
  alarm?: Alarm;
  /** Persistence (default: IndexedDB; `null` disables). */
  persistence?: TimerPersistence | null;
  /** Display refresh interval in ms (default 1000). */
  tickMs?: number;
}

/** Describes a timer's origin for notifications. */
function contextLine(timer: Timer): string {
  const { title, step } = timer.context;
  return step === undefined ? title : `${title} · step ${step}`;
}

/**
 * Owns every kitchen timer: persists them, ticks the display, fires the alarm
 * as each finishes, and stops it once nothing is ringing.
 */
export function TimersProvider({
  children,
  clock = systemClock,
  alarm: alarmProp,
  persistence = storeTimerPersistence,
  tickMs = 1000,
}: TimersProviderProps) {
  const engine = useMemo(() => createTimerEngine(clock), [clock]);
  const alarm = useMemo(() => alarmProp ?? createAlarm(), [alarmProp]);
  const timers = useSyncExternalStore(engine.subscribe, engine.getTimers);
  const [ready, setReady] = useState(persistence === null);
  const [announcement, setAnnouncement] = useState("");
  const loaded = useRef(persistence === null);

  // Load saved timers once, merging any started before loading finished.
  useEffect(() => {
    if (!persistence) return;
    let cancelled = false;
    persistence
      .load()
      .then((saved) => {
        if (cancelled) return;
        const current = engine.getTimers();
        const ids = new Set(current.map((t) => t.id));
        engine.load([...saved.filter((t) => !ids.has(t.id)), ...current]);
        log.info("timers.loaded", { count: saved.length });
      })
      .catch((error: unknown) => {
        log.error("timers.load.failed", { error: String(error) });
      })
      .finally(() => {
        if (cancelled) return;
        loaded.current = true;
        setReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, [engine, persistence]);

  // Save on every change once loaded (never clobber unloaded state).
  useEffect(() => {
    if (!persistence || !loaded.current) return;
    persistence.save(timers).catch((error: unknown) => {
      log.error("timers.save.failed", { error: String(error) });
    });
  }, [persistence, timers]);

  const hasLive = timers.some(
    (t) => t.state === "running" || t.state === "done",
  );
  const now = useNow(tickMs, hasLive, clock);

  // Settle on every tick: ring for each timer that just finished.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `now` is the tick trigger; due() reads the clock itself.
  useEffect(() => {
    if (!ready) return;
    for (const timer of engine.due()) {
      alarm.fire({
        id: timer.id,
        title: `${timer.label} is done`,
        body: contextLine(timer),
      });
      setAnnouncement(`${timer.label} timer is done. ${contextLine(timer)}.`);
    }
  }, [engine, alarm, now, ready]);

  const views = useMemo(
    () => snapshot(timers, Math.max(now, clock.now())),
    [timers, now, clock],
  );

  // Silence the alarm for anything no longer ringing (acknowledged, snoozed,
  // restarted or dismissed).
  useEffect(() => {
    const ringing = new Set(views.filter((v) => v.ringing).map((v) => v.id));
    for (const id of alarm.ringing()) {
      if (!ringing.has(id)) alarm.stop(id);
    }
  }, [views, alarm]);

  const start = useCallback(
    (input: TimerInput) => {
      alarm.prime();
      void requestPersistence();
      return engine.start(input);
    },
    [engine, alarm],
  );

  const api = useMemo<TimersApi>(
    () => ({
      views,
      now: Math.max(now, clock.now()),
      announcement,
      ready,
      start,
      pause: engine.pause,
      resume: engine.resume,
      addTime: engine.addTime,
      dismiss: engine.dismiss,
      restart: engine.restart,
      acknowledge: (id) => {
        engine.acknowledge(id);
        alarm.stop(id);
      },
    }),
    [views, now, clock, announcement, ready, start, engine, alarm],
  );

  return (
    <TimersContext.Provider value={api}>{children}</TimersContext.Provider>
  );
}

/** The kitchen timers. Must be used under {@link TimersProvider}. */
export function useTimers(): TimersApi {
  const api = useContext(TimersContext);
  if (!api) throw new Error("useTimers must be used inside <TimersProvider>");
  return api;
}
