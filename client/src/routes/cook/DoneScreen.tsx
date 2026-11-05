import type { Dispatch, RefObject } from "react";
import { Link } from "react-router-dom";
import { Icon } from "../../components/recipe/Icon";
import { Paragraphs } from "../../components/recipe/RichText";
import { useTranslation } from "../../i18n";
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
  const { t } = useTranslation("cook");
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
            {t("done.title")}
          </h1>
          <p className="cook-done__title">{view.title}</p>
          <dl className="time-summary">
            {view.yield_text && (
              <div>
                <dt>{t("done.makes")}</dt>
                <dd>{view.yield_text}</dd>
              </div>
            )}
            <div>
              <dt>{t("done.steps")}</dt>
              <dd className="tabular">
                {t("done.stepsCount", { completed, total })}
              </dd>
            </div>
          </dl>
          {runningTimers > 0 && (
            <p className="banner warn">
              {t("done.running", { count: runningTimers })}
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
                <Icon name="left" /> {t("done.lastStep")}
              </button>
            )}
            <button
              type="button"
              className="ghost"
              onClick={() => dispatch({ type: "reset" })}
            >
              <Icon name="restart" /> {t("done.cookAgain")}
            </button>
          </div>
        </section>
      </div>
      <nav className="cook-nav cook-nav--single" aria-label={t("nav.cook")}>
        <Link className="button" to={recipeHref}>
          {t("done.back")}
        </Link>
      </nav>
    </>
  );
}
