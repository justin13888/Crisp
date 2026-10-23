import { useState } from "react";
import { Link } from "react-router-dom";
import { useTimers } from "../hooks/useTimers";
import { formatClock, formatDoneAgo } from "../lib/timeFormat";
import type { TimerView } from "../lib/timers";

const ONE_MINUTE = 60_000;

/** "Ragù · step 3" */
function contextText(view: TimerView): string {
  const { title, step } = view.context;
  return step === undefined ? title : `${title} · step ${step}`;
}

/** One timer row. */
function TimerRow({ view }: { view: TimerView }) {
  const { pause, resume, addTime, dismiss, restart, acknowledge } = useTimers();
  const done = view.state === "done";
  const classes = [
    "timer",
    `timer--${view.state}`,
    view.ringing ? "timer--ringing" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <li className={classes} data-timer-id={view.id}>
      <div className="timer__info">
        <div className="timer__label">{view.label}</div>
        <div className="timer__context">
          {view.context.href ? (
            <Link to={view.context.href}>{contextText(view)}</Link>
          ) : (
            contextText(view)
          )}
        </div>
      </div>
      <div className="timer__time">
        <span role="timer">
          {done ? "Done" : formatClock(view.remainingMs)}
        </span>
        {done ? (
          <span className="timer__status">{formatDoneAgo(view.overdueMs)}</span>
        ) : view.state === "paused" ? (
          <span className="timer__status">Paused</span>
        ) : null}
      </div>
      <div className="timer__actions">
        {view.ringing && (
          <button type="button" onClick={() => acknowledge(view.id)}>
            Acknowledge
          </button>
        )}
        {view.state === "running" && (
          <button
            type="button"
            className="secondary"
            onClick={() => pause(view.id)}
          >
            Pause
          </button>
        )}
        {view.state === "paused" && (
          <button type="button" onClick={() => resume(view.id)}>
            Resume
          </button>
        )}
        <button
          type="button"
          className="secondary"
          onClick={() => addTime(view.id, ONE_MINUTE)}
          aria-label={`Add 1 minute to ${view.label}`}
        >
          +1 min
        </button>
        {done && (
          <button
            type="button"
            className="secondary"
            onClick={() => restart(view.id)}
          >
            Restart
          </button>
        )}
        <button
          type="button"
          className="ghost"
          onClick={() => dismiss(view.id)}
          aria-label={`Dismiss ${view.label} timer`}
        >
          Dismiss
        </button>
      </div>
      {!done && (
        <span
          className="timer__progress"
          style={{ width: `${view.progress * 100}%` }}
          aria-hidden="true"
        />
      )}
    </li>
  );
}

/**
 * The global timer dock: every active timer, most urgent first, pinned to the
 * bottom of the screen. Finished timers flash until acknowledged; the dock
 * folds into a compact pill when many timers run (but never hides a ringing
 * one). Finishes are announced through an assertive live region.
 */
export function TimerDock({ collapseAt = 3 }: { collapseAt?: number }) {
  const { views, announcement } = useTimers();
  const [userCollapsed, setUserCollapsed] = useState<boolean | null>(null);
  const ringing = views.filter((v) => v.ringing).length;
  const collapsed =
    ringing === 0 && (userCollapsed ?? views.length >= collapseAt);
  const next = views[0];

  return (
    <>
      <div className="visually-hidden" aria-live="assertive" aria-atomic="true">
        {announcement}
      </div>
      {views.length > 0 && (
        <section className="timer-dock" aria-label="Kitchen timers">
          <div className="timer-dock__inner">
            {collapsed && next ? (
              <button
                type="button"
                className="timer-pill secondary"
                aria-expanded="false"
                onClick={() => setUserCollapsed(false)}
              >
                <span>
                  {views.length} {views.length === 1 ? "timer" : "timers"}
                </span>
                <span>
                  {next.label}{" "}
                  <strong>
                    {next.state === "done"
                      ? "done"
                      : formatClock(next.remainingMs)}
                  </strong>
                </span>
              </button>
            ) : (
              <>
                {views.length > 1 && (
                  <div className="timer-dock__head">
                    <strong>
                      {views.length} timers
                      {ringing > 0 ? ` · ${ringing} done` : ""}
                    </strong>
                    {ringing === 0 && (
                      <button
                        type="button"
                        className="ghost"
                        aria-expanded="true"
                        onClick={() => setUserCollapsed(true)}
                      >
                        Collapse
                      </button>
                    )}
                  </div>
                )}
                <ul className="timer-dock__list">
                  {views.map((view) => (
                    <TimerRow key={view.id} view={view} />
                  ))}
                </ul>
              </>
            )}
          </div>
        </section>
      )}
    </>
  );
}
