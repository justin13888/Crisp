/**
 * The typed boundary to the `crisp-core` WebAssembly module.
 *
 * The module is loaded lazily from `public/crisp-wasm/` (not bundled), and
 * every value it returns arrives as `unknown`. The guards here narrow those
 * values before any UI code touches them — this is the single trust boundary
 * for format and crypto logic, all of which lives in Rust.
 */

import type { WasmIdentityHandle } from "/crisp-wasm/crisp_core.js";
import type {
  BundlePayload,
  ComplianceReport,
  PublicKeySet,
  Recipe,
} from "./types";

let modulePromise: Promise<typeof import("/crisp-wasm/crisp_core.js")> | null =
  null;

/** Loads and instantiates the WASM module exactly once. */
async function core(): Promise<typeof import("/crisp-wasm/crisp_core.js")> {
  if (!modulePromise) {
    modulePromise = (async () => {
      const module = await import(
        /* webpackIgnore: true */ "/crisp-wasm/crisp_core.js"
      );
      await module.default("/crisp-wasm/crisp_core_bg.wasm");
      module.start();
      return module;
    })();
  }
  return modulePromise;
}

/** Narrows an unknown value to a non-null object. */
function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/** Validates a value the WASM boundary claims is a compliance report. */
function asReport(value: unknown): ComplianceReport {
  if (
    isObject(value) &&
    Array.isArray(value["checks"]) &&
    typeof value["compliant"] === "boolean"
  ) {
    return value as unknown as ComplianceReport;
  }
  throw new Error("the verifier returned a malformed compliance report");
}

/** Validates a value the WASM boundary claims is a bundle payload. */
function asPayload(value: unknown): BundlePayload {
  if (
    isObject(value) &&
    Array.isArray(value["recipes"]) &&
    typeof value["doc_id"] === "string"
  ) {
    return value as unknown as BundlePayload;
  }
  throw new Error("the parser returned a malformed bundle payload");
}

/** Validates a value the WASM boundary claims is a recipe. */
function asRecipe(value: unknown): Recipe {
  if (
    isObject(value) &&
    Array.isArray(value["steps"]) &&
    typeof value["id"] === "string"
  ) {
    return value as unknown as Recipe;
  }
  throw new Error("the scaler returned a malformed recipe");
}

/** Validates a value the WASM boundary claims is a public key set. */
function asKeyset(value: unknown): PublicKeySet {
  if (
    isObject(value) &&
    value["ed25519"] instanceof Uint8Array &&
    value["ml-dsa-65"] instanceof Uint8Array
  ) {
    return value as unknown as PublicKeySet;
  }
  throw new Error("the identity returned a malformed public key set");
}

/** A derived author identity. Secret key material stays inside WASM memory. */
export class CrispIdentity {
  readonly #handle: WasmIdentityHandle;
  /** The `usr…` id this identity owns. */
  readonly userId: string;
  /** The public key set — the only key material exposed to JavaScript. */
  readonly publicKeyset: PublicKeySet;

  constructor(handle: WasmIdentityHandle) {
    this.#handle = handle;
    this.userId = handle.userId();
    this.publicKeyset = asKeyset(handle.publicKeyset());
  }

  /** Signs an arbitrary message — used for the server login handshake. */
  signMessage(message: Uint8Array): Uint8Array {
    return this.#handle.signMessage(message);
  }

  /** Author-signs an already-finalized bundle payload. */
  signBundle(payload: BundlePayload, signedAt: number): Uint8Array {
    return this.#handle.signBundle(payload, signedAt);
  }

  /** Builds a `.crisp` file from a draft payload: finalizes every recipe
   * (deriving ids and stamping this author's fingerprint), then signs. The
   * draft's recipe `id`, `revision`, and `author_fpr` fields are ignored. */
  buildBundle(payload: BundlePayload, signedAt: number): Uint8Array {
    return this.#handle.buildBundle(payload, signedAt);
  }

  /** Releases the secret key material held in WASM memory. */
  dispose(): void {
    this.#handle.free();
  }
}

/** The current Unix time in whole seconds — the unit Crisp timestamps use. */
export function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

/** Verifies a `.crisp` file and returns its compliance report. */
export async function verifyBundle(
  bytes: Uint8Array,
  platformKeys: PublicKeySet[] = [],
): Promise<ComplianceReport> {
  const module = await core();
  return asReport(module.verifyBundle(bytes, platformKeys, nowSeconds()));
}

/** Decodes a `.crisp` file into its bundle payload for rendering. */
export async function parseBundle(bytes: Uint8Array): Promise<BundlePayload> {
  const module = await core();
  return asPayload(module.parseBundle(bytes));
}

/** Scales a recipe by an exact `numerator / denominator` factor. */
export async function scaleRecipe(
  recipe: Recipe,
  numerator: number,
  denominator: number,
): Promise<Recipe> {
  const module = await core();
  return asRecipe(module.scaleRecipe(recipe, BigInt(numerator), denominator));
}

/** Derives an author identity from an account username and passphrase. */
export async function deriveIdentity(
  username: string,
  passphrase: string,
): Promise<CrispIdentity> {
  const module = await core();
  return new CrispIdentity(module.deriveIdentity(username, passphrase));
}

/** The format version this build of `crisp-core` implements. */
export async function formatVersion(): Promise<number> {
  return (await core()).crispFormatVersion();
}

/** The `crisp-core` build version string. */
export async function coreVersion(): Promise<string> {
  return (await core()).coreVersion();
}
