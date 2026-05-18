import { useEffect, useState } from "react";
import { formatQuantity } from "../lib/render";
import type { Quantity } from "../lib/types";

/** UCUM time units the timer understands, in seconds. */
const UNIT_SECONDS: Record<string, number> = {
  s: 1,
  min: 60,
  h: 3600,
  d: 86400,
};

/** Converts a duration quantity to whole seconds, or `null` if not a time. */
function durationSeconds(duration: Quantity): number | null {
  if (!duration.min || !duration.unit) return null;
  const factor = UNIT_SECONDS[duration.unit];
  if (factor === undefined) return null;
  const [numerator, denominator] = duration.min;
  if (denominator <= 0) return null;
  return Math.round((numerator / denominator) * factor);
}

/** Formats whole seconds as `mm:ss` (or `h:mm:ss`). */
function clock(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  const mm = m.toString().padStart(2, "0");
  const ss = s.toString().padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

/**
 * A live countdown timer for a step that declares a duration. When the step's
 * duration is not expressed in a time unit, the duration is shown as plain
 * text with no timer.
 */
export function StepTimer({
  duration,
  prefs,
  def,
}: {
  duration: Quantity;
  prefs: string[];
  def: string;
}) {
  const total = durationSeconds(duration);
  const [remaining, setRemaining] = useState(total ?? 0);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    if (!running) return;
    const handle = window.setInterval(() => {
      setRemaining((value) => {
        if (value <= 1) {
          setRunning(false);
          return 0;
        }
        return value - 1;
      });
    }, 1000);
    return () => window.clearInterval(handle);
  }, [running]);

  if (total === null) {
    return (
      <p>
        <strong>Duration:</strong> {formatQuantity(duration, prefs, def)}
      </p>
    );
  }

  return (
    <div className="row" style={{ alignItems: "center" }}>
      <strong style={{ fontVariantNumeric: "tabular-nums" }}>
        {clock(remaining)}
      </strong>
      <button
        type="button"
        onClick={() => setRunning((value) => !value)}
        disabled={remaining === 0}
      >
        {running ? "Pause" : "Start"}
      </button>
      <button
        type="button"
        className="secondary"
        onClick={() => {
          setRunning(false);
          setRemaining(total);
        }}
      >
        Reset
      </button>
      {remaining === 0 && <span className="badge ok">Done</span>}
    </div>
  );
}
