/**
 * The timer alarm: a repeating Web Audio beep, a vibration pattern, and a
 * system notification — each feature-detected and failure-tolerant, so a
 * missing or failing channel never prevents the others.
 *
 * Browsers only let audio start after a user gesture, so call
 * {@link Alarm.prime} from the click that starts the first timer: it creates
 * (or resumes) the `AudioContext` and asks for notification permission.
 *
 * Every browser API sits behind {@link AlarmDeps}, so tests drive the alarm
 * with fakes.
 */

import { createLogger } from "./log";

const log = createLogger("alarm");

/** A minimal tone generator over Web Audio (the testable audio seam). */
export interface ToneOutput {
  /** `suspended` until a user gesture resumes it. */
  readonly state: string;
  /** Resumes a suspended output. */
  resume(): Promise<void>;
  /** The output clock in seconds. */
  now(): number;
  /** Plays a square-wave tone of `hz` from `start` for `duration` seconds. */
  tone(hz: number, start: number, duration: number): void;
}

/** Notification permission, plus `unsupported`. */
export type NotifyPermission = NotificationPermission | "unsupported";

/** A system-notification channel. */
export interface Notifier {
  /** Current permission. */
  permission(): NotifyPermission;
  /** Asks for permission (call from a user gesture). */
  requestPermission(): Promise<NotifyPermission>;
  /** Shows (or replaces) the notification with `tag`. */
  show(tag: string, title: string, body: string): Promise<void>;
  /** Closes the notification with `tag`. */
  close(tag: string): Promise<void>;
}

/** Everything the alarm touches in the browser. */
export interface AlarmDeps {
  /** Creates the audio output, or `null` when Web Audio is unavailable. */
  createAudio: (() => ToneOutput) | null;
  /** `navigator.vibrate`, or `null`. */
  vibrate: ((pattern: number | number[]) => boolean) | null;
  /** System notifications, or `null`. */
  notifier: Notifier | null;
  /** Repeating scheduler (defaults to `setInterval`). */
  every(fn: () => void, ms: number): () => void;
}

/** What to announce when a timer finishes. */
export interface AlarmInfo {
  /** The timer id (also the notification tag suffix). */
  id: string;
  /** Notification title, e.g. "Simmer sauce is done". */
  title: string;
  /** Notification body, e.g. "Ragù · step 3". */
  body: string;
}

/** A timer alarm. */
export interface Alarm {
  /** Unlocks audio and requests notification permission (user gesture). */
  prime(): void;
  /** Starts ringing for a finished timer (idempotent per id). */
  fire(info: AlarmInfo): void;
  /** Stops ringing for one timer, or for all when `id` is omitted. */
  stop(id?: string): void;
  /** Ids currently ringing. */
  ringing(): string[];
}

/** One beep cycle: three short tones. */
export const BEEP = { hz: 880, onMs: 150, offMs: 100, count: 3 } as const;
/** How often the beep cycle (and vibration) repeats. */
export const CYCLE_MS = 2000;
/** The vibration pattern per cycle. */
export const VIBRATION = [400, 200, 400];

/** The notification tag for a timer. */
export function notificationTag(id: string): string {
  return `crisp-timer-${id}`;
}

function attempt(event: string, fn: () => void): void {
  try {
    fn();
  } catch (error) {
    log.warn(event, { error: String(error) });
  }
}

/** Schedules one cycle of beeps on `out`. */
export function scheduleBeeps(out: ToneOutput): void {
  const t0 = out.now() + 0.01;
  for (let i = 0; i < BEEP.count; i++) {
    const start = t0 + (i * (BEEP.onMs + BEEP.offMs)) / 1000;
    out.tone(BEEP.hz, start, BEEP.onMs / 1000);
  }
}

/** Creates an alarm over the given (or the browser's) capabilities. */
export function createAlarm(deps: AlarmDeps = browserAlarmDeps()): Alarm {
  let ctx: ToneOutput | null = null;
  let cancelLoop: (() => void) | null = null;
  const active = new Set<string>();

  const audio = (): ToneOutput | null => {
    if (!ctx && deps.createAudio) {
      try {
        ctx = deps.createAudio();
        log.debug("audio.created", { state: ctx.state });
      } catch (error) {
        log.warn("audio.create.failed", { error: String(error) });
      }
    }
    if (ctx && ctx.state === "suspended") {
      ctx.resume().catch((error: unknown) => {
        log.debug("audio.resume.failed", { error: String(error) });
      });
    }
    return ctx;
  };

  const cycle = (): void => {
    const context = audio();
    if (context) attempt("audio.beep.failed", () => scheduleBeeps(context));
    if (deps.vibrate) {
      const vibrate = deps.vibrate;
      attempt("vibrate.failed", () => vibrate(VIBRATION));
    }
  };

  const silence = (): void => {
    cancelLoop?.();
    cancelLoop = null;
    if (deps.vibrate) {
      const vibrate = deps.vibrate;
      attempt("vibrate.cancel.failed", () => vibrate(0));
    }
  };

  return {
    prime: () => {
      audio();
      const notifier = deps.notifier;
      if (notifier && notifier.permission() === "default") {
        notifier
          .requestPermission()
          .then((permission) => log.info("notify.permission", { permission }))
          .catch((error: unknown) =>
            log.warn("notify.permission.failed", { error: String(error) }),
          );
      }
    },
    fire: (info) => {
      if (active.has(info.id)) return;
      active.add(info.id);
      log.info("alarm.fire", { id: info.id, ringing: active.size });
      if (!cancelLoop) {
        cycle();
        cancelLoop = deps.every(cycle, CYCLE_MS);
      }
      const notifier = deps.notifier;
      if (notifier && notifier.permission() === "granted") {
        notifier
          .show(notificationTag(info.id), info.title, info.body)
          .catch((error: unknown) =>
            log.warn("notify.show.failed", { error: String(error) }),
          );
      }
    },
    stop: (id) => {
      const ids = id === undefined ? [...active] : active.has(id) ? [id] : [];
      for (const each of ids) {
        active.delete(each);
        deps.notifier
          ?.close(notificationTag(each))
          .catch((error: unknown) =>
            log.debug("notify.close.failed", { error: String(error) }),
          );
      }
      if (ids.length > 0) log.info("alarm.stop", { ids, ringing: active.size });
      if (active.size === 0) silence();
    },
    ringing: () => [...active],
  };
}

/** The browser `Notification` channel, preferring the service worker. */
export function browserNotifier(): Notifier | null {
  if (typeof Notification === "undefined") return null;
  const shown = new Map<string, Notification>();
  const registration = async (): Promise<ServiceWorkerRegistration | null> =>
    (await globalThis.navigator?.serviceWorker?.getRegistration?.()) ?? null;
  return {
    permission: () => Notification.permission,
    requestPermission: () => Notification.requestPermission(),
    show: async (tag, title, body) => {
      const options: NotificationOptions = {
        tag,
        body,
        icon: "/icon-192.png",
        requireInteraction: true,
      };
      // Mobile browsers only allow notifications via a service worker.
      const reg = await registration();
      if (reg) {
        await reg.showNotification(title, options);
        return;
      }
      shown.set(tag, new Notification(title, options));
    },
    close: async (tag) => {
      shown.get(tag)?.close();
      shown.delete(tag);
      const reg = await registration();
      for (const n of (await reg?.getNotifications({ tag })) ?? []) n.close();
    },
  };
}

/** Wraps a real `AudioContext` as a {@link ToneOutput}. */
export function webAudioOutput(ctx: AudioContext): ToneOutput {
  return {
    get state() {
      return ctx.state;
    },
    resume: () => ctx.resume(),
    now: () => ctx.currentTime,
    tone: (hz, start, duration) => {
      const gain = ctx.createGain();
      gain.gain.setValueAtTime(0.4, start);
      gain.gain.setValueAtTime(0, start + duration);
      gain.connect(ctx.destination);
      const osc = ctx.createOscillator();
      osc.type = "square";
      osc.frequency.setValueAtTime(hz, start);
      osc.connect(gain);
      osc.start(start);
      osc.stop(start + duration);
    },
  };
}

/** Real browser capabilities, each feature-detected. */
export function browserAlarmDeps(): AlarmDeps {
  const AudioCtor: (new () => AudioContext) | undefined =
    typeof AudioContext === "undefined" ? undefined : AudioContext;
  const nav: Navigator | undefined = globalThis.navigator;
  return {
    createAudio: AudioCtor ? () => webAudioOutput(new AudioCtor()) : null,
    vibrate:
      nav && typeof nav.vibrate === "function"
        ? (pattern) => nav.vibrate(pattern)
        : null,
    notifier: browserNotifier(),
    every: (fn, ms) => {
      const handle = setInterval(fn, ms);
      return () => clearInterval(handle);
    },
  };
}
