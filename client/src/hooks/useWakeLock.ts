import { useEffect, useMemo, useState } from "react";
import { createLogger } from "../lib/log";

const log = createLogger("wake-lock");

/** The slice of `WakeLockSentinel` used. */
export interface SentinelLike {
  /** Whether the lock has been released. */
  readonly released: boolean;
  /** Releases the lock. */
  release(): Promise<void>;
  /** Fires `release` when the browser drops the lock (e.g. tab hidden). */
  addEventListener(type: "release", listener: () => void): void;
}

/** What the controller needs from the browser. */
export interface WakeLockDeps {
  /** Requests a screen lock, or `null` when unsupported. */
  request: (() => Promise<SentinelLike>) | null;
  /** Whether the page is visible (locks are only granted when visible). */
  isVisible(): boolean;
}

/** Drives a screen wake lock from a desired state. */
export interface WakeLockController {
  /** Whether the Screen Wake Lock API is available. */
  readonly supported: boolean;
  /** Sets whether the screen should stay awake. */
  setWanted(wanted: boolean): Promise<void>;
  /** Call on `visibilitychange`: re-acquires a lock the browser dropped. */
  onVisibilityChange(): Promise<void>;
  /** Whether a lock is currently held. */
  isActive(): boolean;
}

/**
 * Creates a wake-lock controller. The browser releases the lock whenever the
 * page is hidden; the controller re-acquires it when the page is visible
 * again and the lock is still wanted. `onChange` reports the held state.
 */
export function createWakeLockController(
  deps: WakeLockDeps,
  onChange: (active: boolean) => void = () => undefined,
): WakeLockController {
  let wanted = false;
  let sentinel: SentinelLike | null = null;
  let pending: Promise<void> | null = null;

  const acquire = async (): Promise<void> => {
    if (!deps.request || sentinel || pending || !deps.isVisible()) return;
    const request = deps.request;
    pending = (async () => {
      try {
        const lock = await request();
        if (!wanted) {
          await lock.release();
          return;
        }
        sentinel = lock;
        lock.addEventListener("release", () => {
          if (sentinel === lock) sentinel = null;
          log.debug("released", {});
          onChange(false);
        });
        log.debug("acquired", {});
        onChange(true);
      } catch (error) {
        log.warn("request.failed", { error: String(error) });
      } finally {
        pending = null;
      }
    })();
    await pending;
  };

  const release = async (): Promise<void> => {
    const lock = sentinel;
    sentinel = null;
    if (lock && !lock.released) {
      try {
        await lock.release();
      } catch (error) {
        log.debug("release.failed", { error: String(error) });
      }
    }
    if (lock) onChange(false);
  };

  return {
    supported: deps.request !== null,
    setWanted: async (next) => {
      wanted = next;
      if (next) await acquire();
      else await release();
    },
    onVisibilityChange: async () => {
      if (wanted && deps.isVisible()) await acquire();
    },
    isActive: () => sentinel !== null && !sentinel.released,
  };
}

/** The browser's wake-lock capabilities. */
export function browserWakeLockDeps(): WakeLockDeps {
  const wakeLock = globalThis.navigator?.wakeLock;
  return {
    request: wakeLock ? () => wakeLock.request("screen") : null,
    isVisible: () => document.visibilityState === "visible",
  };
}

/**
 * Keeps the screen on while `active` (e.g. in cook mode), re-acquiring the
 * lock whenever the page becomes visible again.
 */
export function useWakeLock(
  active: boolean,
  deps?: WakeLockDeps,
): { supported: boolean; active: boolean } {
  const [held, setHeld] = useState(false);
  const controller = useMemo(
    () => createWakeLockController(deps ?? browserWakeLockDeps(), setHeld),
    [deps],
  );

  useEffect(() => {
    void controller.setWanted(active);
    if (!active) return;
    const onVisibility = (): void => {
      void controller.onVisibilityChange();
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      void controller.setWanted(false);
    };
  }, [controller, active]);

  return { supported: controller.supported, active: held };
}
