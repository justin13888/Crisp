/// Ambient declarations for non-code assets and the generated WASM module.

declare module "*.css";

declare module "*.svg" {
  const url: string;
  export default url;
}

/**
 * The `crisp-core` WebAssembly module.
 *
 * It is produced by `just wasm` into `public/crisp-wasm/` and loaded at
 * runtime (not bundled), so this declaration is the type contract the client
 * holds it to. Every value crossing this boundary is treated as untrusted
 * and validated in `lib/crisp.ts` before use.
 */
declare module "*/crisp_core.js" {
  /** Instantiates the module from the `.wasm` binary URL. */
  export default function init(input?: string | URL): Promise<unknown>;
  /** Installs the Rust panic hook. */
  export function start(): void;
  /** The format version this build implements (e.g. `65536`). */
  export function crispFormatVersion(): number;
  /** The lowest bundle `min_v` this build can open. */
  export function minReaderVersion(): number;
  /** The `crisp-core` crate version string. */
  export function coreVersion(): string;
  /** Verifies a `.crisp` file, returning its compliance report. */
  export function verifyBundle(
    bytes: Uint8Array,
    platformKeys: unknown,
    verifiedAt: number,
  ): unknown;
  /** Decodes a `.crisp` file into its bundle payload. */
  export function parseBundle(bytes: Uint8Array): unknown;
  /** Verifies a `signed-profile` and returns the account profile. */
  export function verifyProfile(bytes: Uint8Array): unknown;
  /** Scales a recipe by the rational `numerator / denominator`. */
  export function scaleRecipe(
    recipe: unknown,
    numerator: bigint,
    denominator: number,
  ): unknown;
  /** Returns the numeric limits and closed-enum value lists a recipe
   * authoring UI must respect. The shape is validated by `asConstraints` in
   * `lib/crisp.ts` before any UI code touches it. */
  export function recipeConstraints(): unknown;
  /** Derives an opaque author identity handle from credentials. */
  export function deriveIdentity(
    username: string,
    passphrase: string,
  ): WasmIdentityHandle;

  /** An opaque handle holding live secret key material inside WASM memory. */
  export interface WasmIdentityHandle {
    /** The `usr…` id this key owns. */
    userId(): string;
    /** The public key set as a plain object. */
    publicKeyset(): unknown;
    /** Signs an arbitrary message — used for the login-challenge handshake. */
    signMessage(message: Uint8Array): Uint8Array;
    /** Builds a self-signed `signed-profile` binding a username and display
     * name to this key. */
    signProfile(
      username: string,
      displayName: string,
      signedAt: number,
    ): Uint8Array;
    /** Author-signs an already-finalized bundle payload. */
    signBundle(payload: unknown, signedAt: number): Uint8Array;
    /** Finalizes a draft payload's recipes, then author-signs it. */
    buildBundle(payload: unknown, signedAt: number): Uint8Array;
    /** Releases the secret key material. */
    free(): void;
  }
}
