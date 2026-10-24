import { useCallback } from "react";
import { useEngine } from "../../hooks/useEngine";
import { useTimers } from "../../hooks/useTimers";
import {
  preferredLanguages,
  type RecipeTimerView,
  type StepView,
} from "../../lib/crisp";
import { createLogger } from "../../lib/log";
import { formatClock } from "../../lib/timeFormat";
import { Icon } from "./Icon";
import {
  durationText,
  stepTimerId,
  timerDurationMs,
  timerLabel,
  timerRangeHint,
} from "./model";

const log = createLogger("recipe");

const ONE_MINUTE = 60_000;

/** Identifies one timer of one step. */
export interface StepTimerTarget {
  /** The document id. */
  docId: string;
  /** The recipe title (shown in the timer dock). */
  title: string;
  /** The step the timer belongs to. */
  step: Pick<StepView, "ref" | "number">;
  /** The timer's index within the step. */
  index: number;
  /** The engine's timer view. */
  timer: RecipeTimerView;
}

/**
 * Starts (or, when already running, leaves alone) a step's timer. Returns a
 * stable callback; the caller must invoke it from a user gesture.
 */
export function useStartStepTimer(): (target: StepTimerTarget) => void {
  const { start, views } = useTimers();
  return useCallback(
    ({ docId, title, step, index, timer }: StepTimerTarget) => {
      const id = stepTimerId(docId, step.ref, index);
      const existing = views.find((v) => v.id === id);
      if (existing && existing.state !== "done") return;
      log.info("step.timer.start", { docId, step: step.number, index, id });
      start({
        id,
        label: timerLabel(timer),
        context: { title, step: step.number, href: `/cook/${docId}` },
        durationMs: timerDurationMs(timer),
      });
    },
    [start, views],
  );
}

/**
 * A step's timer: a start button showing its duration until started, then a
 * live countdown with pause/resume and +1 min, then a flashing "Done" until
 * acknowledged. `size="lg"` is the cook-mode variant.
 */
export function StepTimer({
  target,
  size = "sm",
  hotkey,
}: {
  target: StepTimerTarget;
  size?: "sm" | "lg";
  hotkey?: number;
}) {
  const timers = useTimers();
  const startTimer = useStartStepTimer();
  const { docId, step, index, timer } = target;
  const id = stepTimerId(docId, step.ref, index);
  const view = timers.views.find((v) => v.id === id);
  const { engine } = useEngine();
  const label = timerLabel(timer);
  const hint =
    engine &&
    timerRangeHint(timer, (seconds) =>
      durationText(seconds, engine, preferredLanguages()[0] ?? "en"),
    );
  const kind = timer.passive ? "Hands-off" : null;

  if (!view) {
    return (
      <div className={`step-timer step-timer--${size} step-timer--idle`}>
        <button
          type="button"
          className={size === "lg" ? "step-timer__start" : "chip"}
          onClick={() => startTimer(target)}
          aria-label={`Start ${label} timer, ${timer.text}`}
          aria-keyshortcuts={hotkey === undefined ? undefined : String(hotkey)}
        >
          <Icon name="timer" className="step-timer__icon" />
          <span className="step-timer__label">{label}</span>
          <span className="step-timer__duration tabular">{timer.text}</span>
          {size === "lg" && hotkey !== undefined && (
            <kbd className="step-timer__key">{hotkey}</kbd>
          )}
        </button>
        {size === "lg" && (hint || kind) && (
          <span className="step-timer__hint muted">
            {[kind, hint].filter(Boolean).join(" · ")}
          </span>
        )}
      </div>
    );
  }

  const done = view.state === "done";
  const classes = [
    "step-timer",
    `step-timer--${size}`,
    `step-timer--${view.state}`,
    view.ringing ? "step-timer--ringing" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div className={classes} data-timer-id={id}>
      <div className="step-timer__live">
        <span className="step-timer__label">{label}</span>
        <span className="step-timer__clock tabular" role="timer">
          {done ? "Done" : formatClock(view.remainingMs)}
        </span>
        {view.state === "paused" && (
          <span className="step-timer__status">Paused</span>
        )}
      </div>
      <div className="step-timer__actions">
        {view.ringing && (
          <button type="button" onClick={() => timers.acknowledge(id)}>
            Got it
          </button>
        )}
        {view.state === "running" && (
          <button
            type="button"
            className="secondary"
            onClick={() => timers.pause(id)}
          >
            Pause
          </button>
        )}
        {view.state === "paused" && (
          <button type="button" onClick={() => timers.resume(id)}>
            Resume
          </button>
        )}
        <button
          type="button"
          className="secondary"
          onClick={() => timers.addTime(id, ONE_MINUTE)}
          aria-label={`Add 1 minute to ${label}`}
        >
          +1 min
        </button>
        {done && !view.ringing && (
          <button
            type="button"
            className="ghost"
            onClick={() => timers.dismiss(id)}
            aria-label={`Clear ${label} timer`}
          >
            Clear
          </button>
        )}
      </div>
      {!done && (
        <span
          className="step-timer__progress"
          style={{ width: `${view.progress * 100}%` }}
          aria-hidden="true"
        />
      )}
    </div>
  );
}
