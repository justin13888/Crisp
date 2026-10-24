# Crisp

A lightning-fast, offline-first cookbook for recording and sharing recipes.

## Layout

A monorepo over one shared format engine:

- `core/` — Rust crate `crisp-core`: the single implementation of the Crisp
  format — canonical CBOR, Composite ML-DSA signatures, identities and key
  files, the A–E verification pipeline, authoring (seal/revise/fork/endorse),
  and rendering (scaling, units, formatting, the recipe view).
- `server/` — Rust crate `crisp-server`: a [kynos](https://crates.io/crates/kynos)
  + PostgreSQL backend that stores, verifies, endorses, and indexes bundles.
  A thin application layer over `crisp-core`. Its OpenAPI 3.2 document
  (`spec/openapi.json`) is generated from its routes.
- `wasm/` — Rust crate `crisp-wasm`: `crisp-core` plus the
  [spargen](https://crates.io/crates/spargen)-generated client for
  `spec/openapi.json`, compiled to WebAssembly. It is the client's entire
  engine; its TypeScript declarations are generated from the Rust types into
  `client/src/generated/crisp.d.ts`.
- `client/` — the web app: an rsbuild + React + TypeScript PWA. A thin UI
  layer over `crisp-wasm` — no format, crypto, unit, or protocol logic in TS.
- `spec/` — the Crisp format: `crisp-v1.cddl` (structure, normative),
  `README.md` (semantics), `compliance/` (generated test suite), and
  `openapi.json` (generated server contract). Treat it as the source of truth.

`core`, `server`, and `wasm` form a Cargo workspace at the repo root;
`client` is toolchained independently with Bun.

## Code Style

- Contract-driven development: Define the interfaces and data structures first, along with all test cases, before implementing the actual logic.
- Cohesion: All code should be split into cohesive modules that have a single responsibility and clear interfaces. Encapsulate unnecessary details.
- Minimalism: Choose to use a dependency if it reduces the scope of testing and quantity of code and as long as it does not compromise on performance and required capabilities.
- Traceability: all critical processes are verbosely logged so it is clear what happened after the fact and recovery can be feasible. Use INFO logs where necessary and DEBUG,TRACE aggressively for all critical processes. Logs should be structured and easily queryable. Instrument hot paths (e.g. major functions) for performance monitoring and debugging in production.
- Mocking: Use mocks for all external dependencies and critical internal processes. This allows us to have deterministic tests and easily simulate edge cases and failure scenarios that are hard to reproduce with real dependencies. Do not try to wire up two incomplete complex systems to mock each other.

## Quality

Validate the Rust workspace (`core/` + `server/` + `wasm/`):

```bash
just test               # correctness (cargo test --workspace)
just format-check       # formatting
just lint               # lint (clippy -D warnings)
just coverage           # coverage report
just wasm               # build crisp-wasm + refresh client/src/generated
just wasm-test          # crisp-wasm tests under Node
just openapi            # regenerate spec/openapi.json after changing routes
just compliance         # regenerate spec/compliance after changing the format
just mutants [base]     # mutation-test Rust changed since base (cargo-mutants)
just file-size          # per-package line thresholds (core 614, server 456, wasm 400, client 415)
```

`just file-size` fails when a file in `core/`, `server/`, `wasm/`, or
`client/` exceeds its package's line threshold, unless
`scripts/file-size-baseline.txt` lists it. The baseline only shrinks: a
listed file may not grow past its recorded count, and an entry whose file is
back within the threshold must be removed. Split a long file rather than
adding it there.

Validate the client (`client/`):

```bash
bun test                # correctness
bun run typecheck       # tsc --noEmit (strict)
bun run build           # production build
bun x biome format .    # formatting
bun x biome lint .      # lint
bun run e2e             # Playwright end-to-end (phone + tablet)
```

The client loads `crisp-wasm` from `client/public/crisp-wasm/` — run
`just wasm` to (re)generate it. That directory is a git-ignored build
artifact; `client/src/generated/crisp.d.ts` is its committed declaration file
and CI fails if it is stale.

## Code quality rules

- **Core / server (Rust):** All `pub` items need doc comments. Return `Result` for fallible operations — no `unwrap()`/`expect()` outside tests and `main`. `cargo clippy` runs with `-D warnings`; keep it clean.
- **Client (TypeScript):** No `any` — use `unknown` with type guards. `tsconfig.json` strict mode plus `noUnused*` flags are on; keep them passing. Untrusted input (files, network bodies) enters only as bytes and is decoded and verified in Rust; values returned by `crisp-wasm` are typed by the generated declarations. Data the client reads back from its own storage is narrowed with a guard before use.
- Keep the data model aligned with `spec/` — a format change updates the CDDL, `spec/README.md`, `core/`, the compliance suite (`just compliance`), and the generated declarations (`just wasm`) in the same change. A server route change regenerates `spec/openapi.json` (`just openapi`).
