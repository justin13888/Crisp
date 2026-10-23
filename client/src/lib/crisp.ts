/**
 * The client's engine: the `crisp-wasm` module (crisp-core plus the
 * generated server client), loaded lazily from `public/crisp-wasm/`.
 *
 * Every format, crypto, verification, rendering, and network-protocol rule
 * lives in Rust. Values returned by the engine are typed by declarations
 * generated from the Rust types (`src/generated/crisp.d.ts`) and were
 * validated in Rust before they crossed the boundary; untrusted input only
 * ever enters as bytes (files and HTTP bodies), which Rust decodes.
 */

import type * as Wasm from "../generated/crisp";
import { createLogger } from "./log";

export type {
  BlobData,
  BundlePayload,
  BundleSummary,
  Challenge,
  CheckResult,
  ComplianceReport,
  Constraints,
  CrispFailure,
  EquipmentView,
  ImageBlob,
  IngredientSection,
  IngredientView,
  Issue,
  LabeledValue,
  NumberStyle,
  ParsedAmount,
  ParsedTemperature,
  PlatformKey,
  Quantity,
  Rational,
  Recipe,
  RecipeView,
  RemotePage,
  RemoteRecipe,
  RunView,
  SearchQuery,
  ServerInfoView,
  Session,
  StepIngredientView,
  StepView,
  TimerView as RecipeTimerView,
  Trust,
  UnitInfo,
  Units,
  ViewOptions,
} from "../generated/crisp";
export type { Bundle, CrispServer, Signer } from "../generated/crisp";

/** The loaded engine module. */
export type Engine = typeof Wasm;

const log = createLogger("engine");

/** Where `just wasm` puts the module (served verbatim from `public/`). */
const MODULE_URL = "/crisp-wasm/crisp.js";
const WASM_URL = "/crisp-wasm/crisp_bg.wasm";

let enginePromise: Promise<Engine> | null = null;

/** Loads and instantiates the engine exactly once. */
export function loadEngine(): Promise<Engine> {
  if (!enginePromise) {
    const started = performance.now();
    enginePromise = (async () => {
      const module: Engine = await import(/* webpackIgnore: true */ MODULE_URL);
      await module.default({ module_or_path: WASM_URL });
      module.start();
      log.info("engine.ready", {
        ms: Math.round(performance.now() - started),
        ...module.coreInfo(),
      });
      return module;
    })().catch((error: unknown) => {
      enginePromise = null;
      log.error("engine.failed", { error: String(error) });
      throw error;
    });
  }
  return enginePromise;
}

/** Whether `error` is a failure thrown by the engine. */
export function isCrispFailure(error: unknown): error is Wasm.CrispFailure {
  return error instanceof Error && error.name === "CrispFailure";
}

/** A human-readable message for any thrown value. */
export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}

/** Epoch seconds, as the engine expects for timestamps. */
export function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

/** The reader's preferred languages, best first. */
export function preferredLanguages(): string[] {
  const languages = navigator.languages.length
    ? navigator.languages
    : [navigator.language];
  const tags = new Set<string>();
  for (const tag of languages) {
    tags.add(tag);
    const primary = tag.split("-")[0];
    if (primary) tags.add(primary);
  }
  return [...tags];
}
