import { useState } from "react";
import { Link } from "react-router-dom";
import { useLocation } from "react-router-dom";
import { timerContextText, useTimers } from "../hooks/useTimers";
import { useTranslation } from "../i18n";
import { formatClock, formatDoneAgo } from "../lib/timeFormat";
import type { TimerView } from "../lib/timers";

const ONE_MINUTE = 60_000;

/** One timer row. */
function TimerRow({ view }: { view: TimerView }) {
  const { t } = useTranslation("cook");
  const { pause, resume, addTime, dismiss, restart, acknowledge } = useTimers();
  const context = timerContextText(view.context, t);
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
            <Link to={view.context.href}>{context}</Link>
          ) : (
            context
          )}
        </div>
      </div>
      <div className="timer__time">
        <span role="timer">
          {done ? t("timer.done") : formatClock(view.remainingMs)}
        </span>
        {done ? (
          <span className="timer__status">{formatDoneAgo(view.overdueMs)}</span>
        ) : view.state === "paused" ? (
          <span className="timer__status">{t("timer.paused")}</span>
        ) : null}
      </div>
      <div className="timer__actions">
        {view.ringing && (
          <button type="button" onClick={() => acknowledge(view.id)}>
            {t("timer.acknowledge")}
          </button>
        )}
        {view.state === "running" && (
          <button
            type="button"
            className="secondary"
            onClick={() => pause(view.id)}
          >
            {t("timer.pause")}
          </button>
        )}
        {view.state === "paused" && (
          <button type="button" onClick={() => resume(view.id)}>
            {t("timer.resume")}
          </button>
        )}
        <button
          type="button"
          className="secondary"
          onClick={() => addTime(view.id, ONE_MINUTE)}
          aria-label={t("timer.addMinuteLabel", { label: view.label })}
        >
          {t("timer.addMinute")}
        </button>
        {done && (
          <button
            type="button"
            className="secondary"
            onClick={() => restart(view.id)}
          >
            {t("timer.restart")}
          </button>
        )}
        <button
          type="button"
          className="ghost"
          onClick={() => dismiss(view.id)}
          aria-label={t("timer.dismissLabel", { label: view.label })}
        >
          {t("timer.dismiss")}
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
  const { t } = useTranslation("cook");
  const { views: all, announcement } = useTimers();
  // Cook mode shows its own recipe's timers inline; list only the others.
  const cooking = /^\/cook\/([^/]+)/.exec(useLocation().pathname)?.[1];
  const views = cooking
    ? all.filter((v) => !v.id.startsWith(`${decodeURIComponent(cooking)}#`))
    : all;
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
        <section className="timer-dock" aria-label={t("dock.label")}>
          <div className="timer-dock__inner">
            {collapsed && next ? (
              <button
                type="button"
                className="timer-pill secondary"
                aria-expanded="false"
                onClick={() => setUserCollapsed(false)}
              >
                <span>{t("dock.count", { count: views.length })}</span>
                <span>
                  {next.label}{" "}
                  <strong>
                    {next.state === "done"
                      ? t("dock.pillDone")
                      : formatClock(next.remainingMs)}
                  </strong>
                </span>
              </button>
            ) : (
              <>
                {views.length > 1 && (
                  <div className="timer-dock__head">
                    <strong>
                      {t("dock.count", { count: views.length })}
                      {ringing > 0 && (
                        <> · {t("dock.ringing", { count: ringing })}</>
                      )}
                    </strong>
                    {ringing === 0 && (
                      <button
                        type="button"
                        className="ghost"
                        aria-expanded="true"
                        onClick={() => setUserCollapsed(true)}
                      >
                        {t("dock.collapse")}
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
