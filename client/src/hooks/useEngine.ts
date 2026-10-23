import { useEffect, useState } from "react";
import { type Engine, errorMessage, loadEngine } from "../lib/crisp";

/** The engine once loaded, or the reason it failed to load. */
export interface EngineState {
  /** The loaded engine, or `null` while loading or on failure. */
  engine: Engine | null;
  /** Why loading failed, if it did. */
  error: string | null;
}

/** Loads the engine (once per app) and re-renders when it is ready. */
export function useEngine(): EngineState {
  const [state, setState] = useState<EngineState>({
    engine: null,
    error: null,
  });
  useEffect(() => {
    let live = true;
    loadEngine().then(
      (engine) => live && setState({ engine, error: null }),
      (error: unknown) =>
        live && setState({ engine: null, error: errorMessage(error) }),
    );
    return () => {
      live = false;
    };
  }, []);
  return state;
}
