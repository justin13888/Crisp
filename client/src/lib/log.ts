/**
 * A tiny structured logger.
 *
 * Every entry is an event name plus a flat field object, so logs stay
 * queryable (`scope`, `event`, and fields are emitted as one object). The
 * threshold defaults to `info`; set `localStorage["crisp:log"]` to
 * `trace`/`debug`/`info`/`warn`/`error`/`silent` to change it at runtime, or
 * call {@link setLogLevel} (tests silence logs this way).
 */

/** Log severities, from most to least verbose. */
export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

/** A severity threshold; `silent` drops everything. */
export type LogThreshold = LogLevel | "silent";

/** Structured fields attached to a log event. */
export type LogFields = Readonly<Record<string, unknown>>;

/** One structured log entry. */
export interface LogEntry {
  /** Epoch ms when the entry was produced. */
  ts: number;
  /** Severity. */
  level: LogLevel;
  /** The emitting module, e.g. `timers`. */
  scope: string;
  /** A dotted event name, e.g. `timer.started`. */
  event: string;
  /** Event-specific fields. */
  fields: LogFields;
}

/** Receives every entry at or above the threshold. */
export type LogSink = (entry: LogEntry) => void;

/** A scoped logger. */
export interface Logger {
  /** Very fine-grained tracing (hot paths). */
  trace(event: string, fields?: LogFields): void;
  /** Diagnostic detail for critical processes. */
  debug(event: string, fields?: LogFields): void;
  /** Notable state changes. */
  info(event: string, fields?: LogFields): void;
  /** Recoverable problems. */
  warn(event: string, fields?: LogFields): void;
  /** Failures. */
  error(event: string, fields?: LogFields): void;
}

const RANK: Record<LogThreshold, number> = {
  trace: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
  silent: 5,
};

function isThreshold(value: unknown): value is LogThreshold {
  return typeof value === "string" && value in RANK;
}

function storedThreshold(): LogThreshold | null {
  try {
    const value = globalThis.localStorage?.getItem("crisp:log");
    return isThreshold(value) ? value : null;
  } catch {
    return null;
  }
}

const consoleSink: LogSink = (entry) => {
  const method =
    entry.level === "error"
      ? console.error
      : entry.level === "warn"
        ? console.warn
        : entry.level === "info"
          ? console.info
          : console.debug;
  method(`[${entry.scope}] ${entry.event}`, entry);
};

let threshold: LogThreshold | null = null;

/** Overrides the threshold (e.g. `silent` in tests). */
export function setLogLevel(level: LogThreshold): void {
  threshold = level;
}

function enabled(level: LogLevel): boolean {
  const current = threshold ?? storedThreshold() ?? "info";
  return RANK[level] >= RANK[current];
}

/** Creates a logger for one module. */
export function createLogger(scope: string): Logger {
  const emit =
    (level: LogLevel) =>
    (event: string, fields: LogFields = {}): void => {
      if (!enabled(level)) return;
      try {
        consoleSink({ ts: Date.now(), level, scope, event, fields });
      } catch {
        // Logging must never break the app.
      }
    };
  return {
    trace: emit("trace"),
    debug: emit("debug"),
    info: emit("info"),
    warn: emit("warn"),
    error: emit("error"),
  };
}
