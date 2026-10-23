import { useEffect, useState } from "react";
import { type Clock, systemClock } from "../lib/timers";

/**
 * Milliseconds until the next multiple of `intervalMs` — ticks land on whole
 * seconds so every countdown on screen flips together.
 */
export function nextTickDelay(now: number, intervalMs: number): number {
  const remainder = now % intervalMs;
  return remainder === 0 ? intervalMs : intervalMs - remainder;
}

/**
 * The current time, refreshed every `intervalMs` (aligned to the interval)
 * and immediately whenever the page becomes visible again — background tabs
 * throttle timers, so the value re-syncs the moment the cook looks back.
 *
 * Pass `enabled = false` to stop ticking (e.g. when no timer is active).
 */
export function useNow(
  intervalMs = 1000,
  enabled = true,
  clock: Clock = systemClock,
): number {
  const [now, setNow] = useState(() => clock.now());

  useEffect(() => {
    if (!enabled) return;
    let handle: ReturnType<typeof setTimeout> | undefined;
    const tick = (): void => {
      const current = clock.now();
      setNow(current);
      clearTimeout(handle);
      handle = setTimeout(tick, nextTickDelay(current, intervalMs));
    };
    const onVisible = (): void => {
      if (document.visibilityState === "visible") tick();
    };
    tick();
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", onVisible);
    return () => {
      clearTimeout(handle);
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", onVisible);
    };
  }, [intervalMs, enabled, clock]);

  return now;
}
