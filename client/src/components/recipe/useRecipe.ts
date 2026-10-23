import {
  type Dispatch,
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import {
  type CookAction,
  type CookState,
  cookReducer,
  initCookSession,
} from "../../lib/cookSession";
import {
  type Bundle,
  errorMessage,
  preferredLanguages,
  type RecipeView,
} from "../../lib/crisp";
import { type LibraryEntry, openEntry } from "../../lib/library";
import { createLogger } from "../../lib/log";
import { type CookRecord, getStore } from "../../lib/store";
import { fromCookRecord, toCookRecord, viewOptions } from "./model";

const log = createLogger("recipe");

/** A recipe opened from the library, with its saved cook progress. */
export interface LoadedRecipe {
  /** The library entry and its open bundle (freed by the hook). */
  entry: LibraryEntry & { bundle: Bundle };
  /** The revision being viewed (keys the cook progress). */
  revision: string;
  /** How many steps the recipe has. */
  stepCount: number;
  /** Saved cook progress for this revision, if any. */
  saved: CookRecord | undefined;
  /** The reader's languages, best first. */
  languages: string[];
}

/** The state of opening a recipe. */
export type RecipeLoad =
  | { status: "loading" }
  | { status: "missing" }
  | { status: "error"; message: string }
  | ({ status: "ready" } & LoadedRecipe);

/** Opens a library entry and reads its cook progress (`null` if missing). */
export type RecipeLoader = (docId: string) => Promise<LoadedRecipe | null>;

/** The default loader: the library plus the `cook` store. */
export const loadRecipe: RecipeLoader = async (docId) => {
  const started = performance.now();
  const entry = await openEntry(docId);
  if (!entry) return null;
  try {
    const languages = preferredLanguages();
    const revision =
      entry.summary.recipes[0]?.revision ?? entry.record.revision;
    const stepCount = entry.bundle.view(0, { languages }).steps.length;
    const saved = await (await getStore()).cook.get(docId, revision);
    log.info("recipe.open", {
      docId,
      revision,
      stepCount,
      trust: entry.report.trust,
      resumed: Boolean(saved),
      ms: Math.round(performance.now() - started),
    });
    return { entry, revision, stepCount, saved, languages };
  } catch (error: unknown) {
    entry.bundle.free();
    throw error;
  }
};

/**
 * Opens recipe `docId` from the library and frees its bundle when the
 * component unmounts or the id changes.
 */
export function useRecipe(
  docId: string | undefined,
  load: RecipeLoader = loadRecipe,
): RecipeLoad {
  const [state, setState] = useState<RecipeLoad>({ status: "loading" });
  useEffect(() => {
    if (!docId) {
      setState({ status: "missing" });
      return;
    }
    let live = true;
    let opened: Bundle | null = null;
    setState({ status: "loading" });
    load(docId).then(
      (loaded) => {
        if (!live) {
          loaded?.entry.bundle.free();
          return;
        }
        if (!loaded) {
          log.info("recipe.missing", { docId });
          setState({ status: "missing" });
          return;
        }
        opened = loaded.entry.bundle;
        setState({ status: "ready", ...loaded });
      },
      (error: unknown) => {
        log.error("recipe.open.failed", { docId, error: String(error) });
        if (live) setState({ status: "error", message: errorMessage(error) });
      },
    );
    return () => {
      live = false;
      opened?.free();
      opened = null;
    };
  }, [docId, load]);
  return state;
}

/** The rendered view for a session's scale and units. */
export function useRecipeView(
  bundle: Bundle,
  state: Pick<CookState, "scale" | "unitSystem">,
  languages: string[],
): { view: RecipeView | null; error: string | null } {
  const { num, den } = state.scale;
  const units = state.unitSystem;
  return useMemo(() => {
    const started = performance.now();
    try {
      const view = bundle.view(0, viewOptions({ num, den }, units, languages));
      log.debug("view.render", {
        scale: `${num}/${den}`,
        units,
        ms: Math.round((performance.now() - started) * 10) / 10,
      });
      return { view, error: null };
    } catch (error: unknown) {
      log.error("view.failed", {
        scale: `${num}/${den}`,
        units,
        error: String(error),
      });
      return { view: null, error: errorMessage(error) };
    }
  }, [bundle, num, den, units, languages]);
}

/** Saves cook records (injectable for tests). */
export type CookPersist = (record: Omit<CookRecord, "key">) => Promise<void>;

/** The default persistence: the IndexedDB `cook` store. */
export const storeCookPersist: CookPersist = async (record) => {
  await (await getStore()).cook.put(record);
};

/** How long to wait after the last change before saving. */
export const SAVE_DEBOUNCE_MS = 250;

/**
 * A cook session for one recipe revision, restored from `saved` and saved
 * (debounced) on every change. Pending changes are flushed immediately when
 * the page is hidden (phone locked, app switched) or the component unmounts.
 */
export function useCookSession(
  docId: string,
  revision: string,
  stepCount: number,
  saved: CookRecord | undefined,
  persist: CookPersist = storeCookPersist,
): [CookState, Dispatch<CookAction>] {
  const [state, dispatch] = useReducer(
    cookReducer,
    undefined,
    (): CookState => initCookSession(stepCount, fromCookRecord(saved)),
  );
  const initial = useRef(state);
  const pending = useRef<CookState | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const flush = useCallback(() => {
    clearTimeout(timer.current);
    const next = pending.current;
    if (!next) return;
    pending.current = null;
    const record = toCookRecord(docId, revision, next, Date.now());
    log.debug("cook.save", {
      docId,
      revision,
      phase: record.phase,
      step: record.currentStep,
      checked: record.checkedIngredients.length,
    });
    persist(record).catch((error: unknown) => {
      log.error("cook.save.failed", { docId, error: String(error) });
    });
  }, [docId, revision, persist]);

  useEffect(() => {
    if (state === initial.current) return;
    pending.current = state;
    clearTimeout(timer.current);
    timer.current = setTimeout(flush, SAVE_DEBOUNCE_MS);
  }, [state, flush]);

  useEffect(() => {
    const onHide = (): void => {
      if (document.visibilityState === "hidden") flush();
    };
    document.addEventListener("visibilitychange", onHide);
    window.addEventListener("pagehide", flush);
    return () => {
      document.removeEventListener("visibilitychange", onHide);
      window.removeEventListener("pagehide", flush);
      flush();
    };
  }, [flush]);

  const logged = useCallback(
    (action: CookAction) => {
      log.trace("cook.action", { docId, action: action.type });
      dispatch(action);
    },
    [docId],
  );
  return [state, logged];
}
