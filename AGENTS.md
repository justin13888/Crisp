# Crisp

A lightning-fast, offline-first cookbook for recording and sharing recipes.

## Layout

A monorepo of three packages over one shared format engine:

- `core/` — Rust crate `crisp-core`: the single implementation of the Crisp
  format — canonical CBOR, post-quantum Composite ML-DSA signatures, identity
  derivation, and the A–E verification pipeline. It compiles natively (linked
  into the server) and to WebAssembly (consumed by the client), so format
  semantics can never drift between the two.
- `server/` — Rust crate `crisp-server`: an Axum + PostgreSQL backend that
  stores, verifies, endorses, and indexes recipe bundles. A thin application
  layer over `crisp-core`.
- `client/` — the web app: an rsbuild + React + TypeScript PWA that consumes
  `crisp-core` via WASM. A thin UI layer — no format or crypto logic in TS.
- `spec/` — the Crisp recipe bundle format (CDDL). Treat it as the source of
  truth for the data model.

`core` and `server` form a Cargo workspace at the repo root; `client` is
toolchained independently with Bun.

## Code Style

- Contract-driven development: Define the interfaces and data structures first, along with all test cases, before implementing the actual logic.
- Cohesion: All code should be split into cohesive modules that have a single responsibility and clear interfaces. Encapsulate unnecessary details.
- Minimalism: Choose to use a dependency if it reduces the scope of testing and quantity of code and as long as it does not compromise on performance and required capabilities.
- Traceability: all critical processes are verbosely logged so it is clear what happened after the fact and recovery can be feasible. Use INFO logs where necessary and DEBUG,TRACE aggressively for all critical processes. Logs should be structured and easily queryable. Instrument hot paths (e.g. major functions) for performance monitoring and debugging in production.
- Mocking: Use mocks for all external dependencies and critical internal processes. This allows us to have deterministic tests and easily simulate edge cases and failure scenarios that are hard to reproduce with real dependencies. Do not try to wire up two incomplete complex systems to mock each other.

## Quality

Validate the Rust workspace (`core/` + `server/`):

```bash
just test               # correctness (cargo test --workspace)
just format-check       # formatting
just lint               # lint (clippy -D warnings)
just coverage           # coverage report
just wasm               # build core to WASM for the client
```

Validate the client (`client/`):

```bash
bun test                # correctness
bun run typecheck       # tsc --noEmit (strict)
bun run build           # production build
bun x biome format .    # formatting
bun x biome lint .      # lint
```

The client consumes `crisp-core` as a WASM package under
`client/public/crisp-wasm/` — run `just wasm` to (re)generate it. That
directory is a build artifact and is git-ignored.

## Code quality rules

- **Core / server (Rust):** All `pub` items need doc comments. Return `Result` for fallible operations — no `unwrap()`/`expect()` outside tests and `main`. `cargo clippy` runs with `-D warnings`; keep it clean.
- **Client (TypeScript):** No `any` — use `unknown` with type guards. `tsconfig.json` strict mode plus `noUnused*` flags are on; keep them passing. Validate untrusted recipe data at the boundary before trusting its shape — every value crossing the WASM boundary is untrusted until a guard in `lib/crisp.ts` narrows it.
- Keep the data model aligned with `spec/` — changes to the recipe format must update the CDDL spec, `core/`, and the client's `lib/types.ts` in the same change.
