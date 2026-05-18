# Crisp

A lightning-fast, offline-first cookbook for recording and sharing recipes.

## Layout

Monorepo with two independently-toolchained packages:

- `client/` — desktop recipe UI, TypeScript on the Bun runtime.
- `server/` — Rust backend that stores recipe bundles and indexes their metadata for search.
- `spec/` — the Crisp recipe bundle format (CDDL). Treat it as the source of truth for the data model.

## Code Style

- Contract-driven development: Define the interfaces and data structures first, along with all test cases, before implementing the actual logic.
- Cohesion: All code should be split into cohesive modules that have a single responsibility and clear interfaces. Encapsulate unnecessary details.
- Minimalism: Choose to use a dependency if it reduces the scope of testing and quantity of code and as long as it does not compromise on performance and required capabilities.
- Traceability: all critical processes are verbosely logged so it is clear what happened after the fact and recovery can be feasible. Use INFO logs where necessary and DEBUG,TRACE aggressively for all critical processes. Logs should be structured and easily queryable. Instrument hot paths (e.g. major functions) for performance monitoring and debugging in production.
- Mocking: Use mocks for all external dependencies and critical internal processes. This allows us to have deterministic tests and easily simulate edge cases and failure scenarios that are hard to reproduce with real dependencies. Do not try to wire up two incomplete complex systems to mock each other.

## Quality

Validate the client (`client/`):

```bash
bun test                # correctness
bun x biome format .    # formatting
bun x biome lint .      # lint
```

Validate the server (`server/`):

```bash
just test               # correctness
just format-check       # formatting
just lint               # lint
just coverage           # coverage report
```

## Code quality rules

- **Client (TypeScript):** No `any` — use `unknown` with type guards. `tsconfig.json` strict mode plus `noUnused*` flags are on; keep them passing. Validate untrusted recipe data at the boundary before trusting its shape.
- **Server (Rust):** All `pub` items need doc comments. Return `Result` for fallible operations — no `unwrap()`/`expect()` outside tests and `main`. `cargo clippy` runs with `-D warnings`; keep it clean.
- Keep the data model aligned with `spec/` — changes to the recipe format must update the CDDL spec in the same change.
