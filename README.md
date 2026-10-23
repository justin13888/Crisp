# Crisp

A lightning-fast, offline-first cookbook for recording, sharing, and **cooking
from** recipes.

Every recipe is one self-contained, signed `.crisp` file: text, quantities,
timers, temperatures, and photos in a single canonical-CBOR document, signed
with a post-quantum hybrid signature. Files move between devices and people
as-is; a Crisp server can endorse and index them, but nothing needs one.

- **Offline first.** The web app installs as a PWA and works with no network:
  your library, the editor, and cook mode run entirely on the device.
- **Built for the stove.** Cook mode shows one step at a time in large type,
  with the step's ingredients scaled and converted, tap-to-start timers that
  survive a phone lock, a mise-en-place checklist, and the screen kept awake.
- **Yours.** Your identity is a random key that never leaves your device
  except as a passphrase-protected backup file.

## Repository layout

| Path | What | Stack |
| --- | --- | --- |
| [`spec/`](spec/) | The Crisp v1 format — [`crisp-v1.cddl`](spec/crisp-v1.cddl) (structure), [`README.md`](spec/README.md) (semantics, crypto, verification), [`compliance/`](spec/compliance/) (test suite), [`openapi.json`](spec/openapi.json) (server contract) | CDDL, OpenAPI 3.2 |
| [`core/`](core/) | `crisp-core` — the only implementation of the format: canonical CBOR, Composite ML-DSA, identities and key files, the A–E verifier, authoring, scaling/units/formatting | Rust |
| [`server/`](server/) | `crisp-server` — stores, verifies, endorses, and indexes bundles | Rust, [kynos](https://crates.io/crates/kynos), PostgreSQL |
| [`wasm/`](wasm/) | `crisp-wasm` — `crisp-core` plus the [spargen](https://crates.io/crates/spargen)-generated server client, compiled to WebAssembly with generated TypeScript types | Rust → WASM |
| [`client/`](client/) | The web app — a thin UI over `crisp-wasm` | React, TypeScript, rsbuild, Bun |

`core`, `server`, and `wasm` form a Cargo workspace; `client` is toolchained
with Bun. The server's OpenAPI document is generated from its routes and the
client is generated from that document, so the two cannot drift.

## Prerequisites

- [Rust](https://rustup.rs) (`rust-toolchain.toml` pins the channel) and the
  WASM target: `rustup target add wasm32-unknown-unknown`
- `wasm-bindgen-cli` at the version in `Cargo.lock`:
  `cargo install wasm-bindgen-cli --version <version> --locked`
- [Bun](https://bun.sh) and [just](https://github.com/casey/just)
- Docker or Podman, for PostgreSQL (only needed to run a server)
- Optional: [Lefthook](https://github.com/evilmartians/lefthook) (`lefthook install`)

## Getting started

```bash
just wasm                # build the client engine into client/public/crisp-wasm
cd client
bun install
bun run dev              # http://localhost:3000
```

That is all you need to write and cook recipes. To run a server as well:

```bash
cp server/.env.example server/.env   # then set CRISP_PLATFORM_SEED (openssl rand -hex 32)
just dev                             # PostgreSQL via Compose + the server on :8080
```

### Server configuration

| Variable | Required | Default | Meaning |
| --- | --- | --- | --- |
| `CRISP_DATABASE_URL` | yes | — | PostgreSQL URL |
| `CRISP_PLATFORM_SEED` | yes | — | 64 hex chars; the endorsement key's seed — keep it secret and stable |
| `CRISP_BIND_ADDR` | | `0.0.0.0:8080` | Listen address |
| `CRISP_ALLOWED_ORIGINS` | | any | Comma-separated CORS origins |
| `CRISP_MAX_BUNDLE_BYTES` | | 16 MiB | Largest accepted upload |
| `CRISP_SESSION_TTL_SECS` | | 30 days | Session lifetime |
| `CRISP_CHALLENGE_TTL_SECS` | | 5 minutes | Login challenge lifetime |
| `CRISP_PURGE_INTERVAL_SECS` | | 10 minutes | How often expired sessions and challenges are purged |
| `RUST_LOG` | | `info` | Log filter |

## Writing your first recipe

1. **Create your identity** — *Settings → Identity*. Pick a display name and a
   passphrase, then **download the backup** (`crisp-identity.crispkey`). The
   key never leaves the device otherwise; without it you cannot revise your
   recipes as the same author.
2. **New recipe** — type the title, yield (`24` / `cookies`), and times
   (`20 min`, `1h 30m`, `1:30`).
3. **Ingredients** — amounts are parsed as you type: `1 1/2 cups`, `1½ cups`,
   `200 g`, `2-3 tbsp`, `~1 tsp`, `3 cloves`, `to taste`. Paste a whole list
   at once; a line ending in `:` starts a group. Mark salt-for-the-water style
   amounts **fixed** so they never scale, and add an alternative measure
   (`120 g` next to `1 cup`) for metric cooks.
4. **Steps** — one paragraph per step. Attach the ingredients the step uses
   (with a portion, e.g. half the butter), timers (several per step; mark
   oven and resting waits **passive**), heat level, and temperature.
5. **Save** — the draft is sealed into a signed `.crisp` file in your library.
   Editing it later creates a new revision of the same recipe; editing someone
   else's creates your own fork.
6. **Share** — *Export* hands you the `.crisp` file (share sheet on phones);
   *Import* (or drag-and-drop, or "Open with Crisp") adds one.
7. **Cook** — *Start cooking*: tick off the mise en place, then follow one
   step at a time. Swipe or use ←/→ and Space; number keys start a step's
   timers. Progress and timers survive reloads and a locked phone.

## Development

| Command | What it does |
| --- | --- |
| `just test` | All Rust tests (includes the compliance suite and OpenAPI contract checks) |
| `just lint` / `just format-check` | Clippy (`-D warnings`) / rustfmt |
| `just wasm` | Build `crisp-wasm` and refresh `client/src/generated/crisp.d.ts` |
| `just wasm-test` | Run the WASM tests under Node |
| `just openapi` | Regenerate `spec/openapi.json` from the server's routes |
| `just compliance` | Regenerate the compliance suite in `spec/compliance/` |
| `just coverage` | Rust coverage report |
| `just dev` | PostgreSQL + the server |
| `bun test` / `bun run typecheck` / `bun run build` (in `client/`) | Client unit tests, types, production build |
| `bun x biome format .` / `bun x biome lint .` (in `client/`) | Client formatting and lint |
| `bun run e2e` (in `client/`) | Playwright end-to-end suite (phone and tablet) |

CI runs the check behind every command above on every pull request, except
`just dev`, `just openapi`, and `just compliance`; the last two regenerate
checked-in files, which `just test` verifies are current. CI also runs the
file-length gate (`just file-size`).

## License

Crisp is licensed under the GNU AGPL-3.0. Network use of a modified version
requires sharing the corresponding source.
