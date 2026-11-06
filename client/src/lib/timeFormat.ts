/**
 * Human formatting for timer durations — glanceable from across a kitchen.
 */

import { i18n } from "../i18n";

/**
 * Formats a countdown as `mm:ss`, or `h:mm:ss` from one hour. Rounds *up* to
 * the whole second so the display never reads `00:00` while time remains.
 */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = s.toString().padStart(2, "0");
  if (h > 0) return `${h}:${m.toString().padStart(2, "0")}:${ss}`;
  return `${m.toString().padStart(2, "0")}:${ss}`;
}

/**
 * Formats how long ago a timer finished, in the interface language:
 * "Done just now", "Done 3 min ago".
 */
export function formatDoneAgo(overdueMs: number): string {
  const total = Math.floor(Math.max(0, overdueMs) / 60_000);
  if (total < 1) return i18n.t("cook:doneAgo.justNow");
  if (total < 60) return i18n.t("cook:doneAgo.minutes", { minutes: total });
  const hours = Math.floor(total / 60);
  const minutes = total % 60;
  return minutes === 0
    ? i18n.t("cook:doneAgo.hours", { hours })
    : i18n.t("cook:doneAgo.hoursMinutes", { hours, minutes });
}
