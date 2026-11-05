import { useEffect, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
import { Icon } from "../../components/recipe/Icon";
import { stepTimerId } from "../../components/recipe/model";
import { plainText } from "../../components/recipe/RichText";
import { useStartStepTimer } from "../../components/recipe/StepTimer";
import {
  type LoadedRecipe,
  useCookSession,
  useRecipe,
  useRecipeView,
} from "../../components/recipe/useRecipe";
import { useHotkeys } from "../../hooks/useHotkeys";
import { useTimers } from "../../hooks/useTimers";
import { useWakeLock } from "../../hooks/useWakeLock";
import { useTranslation } from "../../i18n";
import { createLogger } from "../../lib/log";
import { RecipeError, RecipeLoading, RecipeNotFound } from "../RecipePage";
import { cookAnnouncement, cookProgressPercent } from "./cookView";
import { DoneScreen } from "./DoneScreen";
import { MiseScreen } from "./MiseScreen";
import { StepDrawer } from "./StepDrawer";
import { StepScreen } from "./StepScreen";
import "../recipe.css";
import "./cook.css";

const log = createLogger("cook");

/** `/cook/:docId` — cook mode. */
export function CookPage() {
  const { docId } = useParams();
  const load = useRecipe(docId);
  switch (load.status) {
    case "loading":
      return <RecipeLoading />;
    case "missing":
      return <RecipeNotFound />;
    case "error":
      return <RecipeError message={load.message} />;
    case "ready":
      return <CookScreen key={load.revision} loaded={load} />;
  }
}

/** Whether router state asks to start over (from "Cook again"). */
function wantsRestart(state: unknown): boolean {
  return (
    typeof state === "object" &&
    state !== null &&
    "restart" in state &&
    state.restart === true
  );
}

function CookScreen({ loaded }: { loaded: LoadedRecipe }) {
  const { t } = useTranslation("cook");
  const { entry, revision, stepCount, saved, languages } = loaded;
  const { bundle } = entry;
  const docId = entry.record.docId;
  const recipeHref = `/recipe/${encodeURIComponent(docId)}`;
  const navigate = useNavigate();
  const location = useLocation();
  const [state, dispatch] = useCookSession(docId, revision, stepCount, saved);
  const { view, error } = useRecipeView(bundle, state, languages);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [keepAwake, setKeepAwake] = useState(true);
  const wake = useWakeLock(keepAwake);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const timers = useTimers();
  const startStepTimer = useStartStepTimer();

  // "Cook again" from the recipe page: start over once, then drop the flag.
  const restartRequested = wantsRestart(location.state);
  useEffect(() => {
    if (!restartRequested) return;
    log.info("cook.restart", { docId, via: "recipe" });
    dispatch({ type: "reset" });
    navigate(location.pathname, { replace: true, state: null });
  }, [restartRequested, dispatch, navigate, location.pathname, docId]);

  useEffect(() => {
    log.info("cook.enter", { docId, revision });
    return () => log.info("cook.exit", { docId });
  }, [docId, revision]);

  // Move focus to the new heading whenever the step or phase changes.
  const position = `${state.phase}:${state.currentStep}`;
  // biome-ignore lint/correctness/useExhaustiveDependencies: `position` is the trigger
  useEffect(() => {
    headingRef.current?.focus({ preventScroll: true });
    log.debug("cook.position", {
      docId,
      phase: state.phase,
      step: state.currentStep,
    });
  }, [position]);

  const step = state.phase === "steps" ? view?.steps[state.currentStep] : null;
  useHotkeys(
    {
      ArrowRight: () => dispatch({ type: "next" }),
      space: () => dispatch({ type: "next" }),
      ArrowLeft: () => dispatch({ type: "prev" }),
      escape: () => navigate(recipeHref),
      ...Object.fromEntries(
        Array.from({ length: 8 }, (_, i) => [
          String(i + 1),
          () => {
            const timer = step?.timers[i];
            if (!step || !timer || !view) return;
            startStepTimer({ docId, title: view.title, step, index: i, timer });
          },
        ]),
      ),
    },
    { enabled: !drawerOpen },
  );

  if (!view) return <RecipeError message={error ?? t("page.unknownError")} />;

  const ownTimers = new Set(
    view.steps.flatMap((s) =>
      s.timers.map((_, i) => stepTimerId(docId, s.ref, i)),
    ),
  );
  const runningTimers = timers.views.filter(
    (v) => ownTimers.has(v.id) && v.state !== "done",
  ).length;

  const percent = cookProgressPercent(state);
  const closeDrawer = (): void => setDrawerOpen(false);

  return (
    <div className="cook cook-mode" data-phase={state.phase}>
      <header className="cook-header">
        <Link className="button ghost cook-exit" to={recipeHref}>
          <Icon name="close" />
          <span>{t("page.exit")}</span>
        </Link>
        <p className="cook-header__title" title={view.title}>
          {view.title}
        </p>
        <div className="cook-header__actions">
          {wake.supported && (
            <button
              type="button"
              className={`ghost icon cook-wake${wake.active ? " is-on" : ""}`}
              aria-pressed={keepAwake}
              aria-label={keepAwake ? t("page.wakeOn") : t("page.wakeOff")}
              title={
                wake.active ? t("page.wakeActive") : t("page.wakeInactive")
              }
              onClick={() => setKeepAwake((v) => !v)}
            >
              <Icon name={keepAwake ? "sun" : "moon"} />
            </button>
          )}
          <button
            type="button"
            className="ghost cook-steps-button"
            onClick={() => setDrawerOpen(true)}
            aria-haspopup="dialog"
          >
            <Icon name="list" />
            <span>{t("page.steps")}</span>
          </button>
        </div>
      </header>
      <div
        className="cook-progress"
        role="progressbar"
        aria-label={t("page.progress")}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent}
      >
        <span style={{ width: `${percent}%` }} />
      </div>
      <div className="visually-hidden" aria-live="polite" aria-atomic="true">
        {cookAnnouncement(state, view.steps, plainText)}
      </div>

      {state.phase === "mise" && (
        <MiseScreen
          view={view}
          bundle={bundle}
          state={state}
          dispatch={dispatch}
          headingRef={headingRef}
          recipeHref={recipeHref}
        />
      )}
      {state.phase === "steps" && (
        <StepScreen
          view={view}
          bundle={bundle}
          docId={docId}
          state={state}
          dispatch={dispatch}
          headingRef={headingRef}
        />
      )}
      {state.phase === "done" && (
        <DoneScreen
          view={view}
          state={state}
          dispatch={dispatch}
          headingRef={headingRef}
          recipeHref={recipeHref}
          runningTimers={runningTimers}
        />
      )}

      {drawerOpen && (
        <StepDrawer
          steps={view.steps}
          state={state}
          actions={{
            close: closeDrawer,
            goTo: (index) => {
              dispatch({ type: "goTo", step: index });
              closeDrawer();
            },
            mise: () => {
              // The reducer reaches mise by stepping back from step 1.
              if (state.phase !== "mise") {
                dispatch({ type: "goTo", step: 0 });
                dispatch({ type: "prev" });
              }
              closeDrawer();
            },
            finish: () => {
              dispatch({ type: "finish" });
              closeDrawer();
            },
            restart: () => {
              log.info("cook.restart", { docId, via: "drawer" });
              dispatch({ type: "reset" });
              closeDrawer();
            },
          }}
        />
      )}
    </div>
  );
}
