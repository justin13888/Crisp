import type { Dispatch, RefObject } from "react";
import { Link } from "react-router-dom";
import { Icon } from "../../components/recipe/Icon";
import { Paragraphs } from "../../components/recipe/RichText";
import type { CookAction, CookState } from "../../lib/cookSession";
import type { RecipeView } from "../../lib/crisp";

/** The finish screen: a summary and the way back. */
export function DoneScreen({
  view,
  state,
  dispatch,
  headingRef,
  recipeHref,
  runningTimers,
}: {
  view: RecipeView;
  state: CookState;
  dispatch: Dispatch<CookAction>;
  headingRef: RefObject<HTMLHeadingElement | null>;
  recipeHref: string;
  runningTimers: number;
}) {
  const total = view.steps.length;
  const completed = state.checkedSteps.length;
  return (
    <>
      <div className="cook-body">
        <section className="cook-done">
          <span className="cook-done__icon" aria-hidden="true">
            <Icon name="check" />
          </span>
          <h1 ref={headingRef} tabIndex={-1} className="cook-title">
            All done!
          </h1>
          <p className="cook-done__title">{view.title}</p>
          <dl className="time-summary">
            {view.yield_text && (
              <div>
                <dt>Makes</dt>
                <dd>{view.yield_text}</dd>
              </div>
            )}
            <div>
              <dt>Steps</dt>
              <dd className="tabular">
                {completed} of {total}
              </dd>
            </div>
          </dl>
          {runningTimers > 0 && (
            <p className="banner warn">
              {runningTimers === 1
                ? "A timer is still running"
                : `${runningTimers} timers are still running`}{" "}
              — keep an eye on the timer bar.
            </p>
          )}
          {view.notes.length > 0 && (
            <div className="callout">
              <Paragraphs paragraphs={view.notes} />
            </div>
          )}
          <div className="cluster cook-done__more">
            {total > 0 && (
              <button
                type="button"
                className="secondary"
                onClick={() => dispatch({ type: "prev" })}
              >
                <Icon name="left" /> Last step
              </button>
            )}
            <button
              type="button"
              className="ghost"
              onClick={() => dispatch({ type: "reset" })}
            >
              <Icon name="restart" /> Cook again
            </button>
          </div>
        </section>
      </div>
      <nav className="cook-nav cook-nav--single" aria-label="Cook navigation">
        <Link className="button" to={recipeHref}>
          Back to recipe
        </Link>
      </nav>
    </>
  );
}
