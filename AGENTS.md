# Crisp

A lightning-fast, offline-first cookbook for recording and sharing recipes.

## Layout

Monorepo with two independently-toolchained packages:

- `client/` — desktop recipe UI, TypeScript on the Bun runtime.
- `server/` — Rust backend that stores recipe bundles and indexes their metadata for search.
- `spec/` — the Crisp recipe bundle format (CDDL). Treat it as the source of truth for the data model.

## Server packages (`server/`)

- **tokio** — async runtime for the HTTP server.
- **tracing** + **tracing-subscriber** — structured request/diagnostic logging.
- **serde** + **serde_json** — (de)serialization for recipe JSON and API payloads.
- **thiserror** — typed library errors.
- **eyre** + **color-eyre** — flexible, colorful error reporting at the application boundary.

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
