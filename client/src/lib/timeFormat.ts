/**
 * Human formatting for timer durations — glanceable from across a kitchen.
 */

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

/** Formats how long ago a timer finished: "Done just now", "Done 3 min ago". */
export function formatDoneAgo(overdueMs: number): string {
  const minutes = Math.floor(Math.max(0, overdueMs) / 60_000);
  if (minutes < 1) return "Done just now";
  if (minutes < 60) return `Done ${minutes} min ago`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m === 0 ? `Done ${h} h ago` : `Done ${h} h ${m} min ago`;
}
