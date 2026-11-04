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

`just dev` and `compose.yaml` are for development only: they use fixed
database credentials and publish PostgreSQL on the host. To run a public
server, follow [`docs/deployment.md`](docs/deployment.md), which covers
[`compose.prod.yaml`](compose.prod.yaml), secrets, platform-key custody,
backups, migrations, and incident runbooks.

### Server configuration

| Variable | Required | Default | Meaning |
| --- | --- | --- | --- |
| `CRISP_DATABASE_URL` | yes | — | PostgreSQL URL |
| `CRISP_DB_MAX_CONNECTIONS` | | `10` | Connection-pool size |
| `CRISP_DB_ACQUIRE_TIMEOUT_SECS` | | `5` | How long a request waits for a pooled connection (including opening one) before it answers 503 |
| `CRISP_DB_STATEMENT_TIMEOUT_SECS` | | `30` | Server-side `statement_timeout` on every connection, migrations included; a cancelled statement answers 503 |
| `CRISP_PLATFORM_SEED` | yes | — | 64 hex chars; the endorsement key's seed — keep it secret and stable ([custody](docs/deployment.md#platform-key-custody)) |
| `CRISP_BIND_ADDR` | | `0.0.0.0:8080` | Listen address |
| `CRISP_ALLOWED_ORIGINS` | | any | Comma-separated CORS origins |
| `CRISP_MAX_BUNDLE_BYTES` | | 16 MiB | Largest accepted upload |
| `CRISP_QUOTA_BYTES_PER_ACCOUNT` | | 256 MiB | Endorsed bytes one account may store across all its revisions; at least `CRISP_MAX_BUNDLE_BYTES`, and set well above it, since the endorsed bundle is larger than the upload. Past it a publish answers 403 `quota_exceeded` |
| `CRISP_QUOTA_BUNDLES_PER_ACCOUNT` | | `5000` | Bundles (revisions) one account may store; past it a publish answers 403 `quota_exceeded` |
| `CRISP_MAX_STORED_BYTES` | | no cap | Endorsed bytes the whole server may store; past it a publish answers 507 `storage_full` |
| `CRISP_VERIFY_CONCURRENCY` | | CPU count | Signature verifications and endorsements run at once; more wait their turn |
| `CRISP_SESSION_TTL_SECS` | | 30 days | Session lifetime |
| `CRISP_CHALLENGE_TTL_SECS` | | 5 minutes | Login challenge lifetime |
| `CRISP_PURGE_INTERVAL_SECS` | | 10 minutes | How often expired sessions and challenges are purged |
| `CRISP_LIMIT_REGISTER` | | `10/3600` | Registrations per client IP, as `REQUESTS/SECONDS` |
| `CRISP_LIMIT_CHALLENGE` | | `10/300` | Login challenges per username |
| `CRISP_LIMIT_CHALLENGE_PEER` | | `30/60` | Login challenges per client IP, any username |
| `CRISP_LIMIT_LOGIN` | | `30/60` | Logins per client IP |
| `CRISP_LIMIT_PUBLISH` | | `60/3600` | Uploads per account |
| `CRISP_LIMIT_QUERY` | | `120/60` | Browse and search requests per client IP, shared |
| `CRISP_TRUSTED_PROXIES` | | none | Comma-separated proxy addresses or `address/prefix` networks whose forwarding header names the client ([trusted proxies](#reverse-proxy-and-client-addresses)) |
| `CRISP_PROXY_HEADER` | | `x-forwarded-for` | The header those proxies write: `x-forwarded-for` or `forwarded` (RFC 7239) |
| `CRISP_HEADER_TIMEOUT_SECS` | | `30` | Time a client has to send a request head; also how long an idle keep-alive connection is kept |
| `CRISP_BODY_IDLE_TIMEOUT_SECS` | | `30` | Time a request body may go without a byte arriving before the request is answered 408 |
| `CRISP_MAX_CONNECTIONS` | | `1024` | Open connections; more wait in the accept backlog |
| `CRISP_MAX_STREAMS_PER_CONNECTION` | | `100` | Concurrent HTTP/2 requests on one connection |
| `CRISP_ADMIN_USR_IDS` | | none | Comma-separated `usr…` ids of the accounts that may call the `/admin` moderation routes ([policy](docs/public-server-policy.md#41-administrators)) |
| `CRISP_ABUSE_CONTACT` | | none | Where to report abuse or send a takedown notice (e-mail or web address), advertised by `GET /server-info`; a WARN is logged at startup when unset |
| `CRISP_TERMS_URL` | | none | The operator's terms of service (`http(s)` URL), advertised by `GET /server-info` and shown at registration; a WARN is logged at startup when unset |
| `RUST_LOG` | | `info` | Log filter |
| `CRISP_LOG_FORMAT` | | `text` | `text` for human-readable lines, `json` for one JSON object per event |
| `CRISP_METRICS_BIND_ADDR` | | `127.0.0.1:9464` | Listen address of the separate, unauthenticated Prometheus listener (`GET /metrics`); `off` disables it; must differ from `CRISP_BIND_ADDR` ([metrics](docs/deployment.md#observability)) |

**Probes and database outages.** `GET /api/v1/health` is the liveness probe:
it answers 200 whenever the process serves and never touches PostgreSQL.
`GET /api/v1/ready` is the readiness probe: 200 while the database answers a
ping, 503 otherwise. While the database is unreachable, overloaded, or a
statement times out, every operation that needs it answers 503
`application/problem+json` with `code: "unavailable"` — never 401, so a
client keeps its session token and retries. At boot the server connects once
and exits non-zero if PostgreSQL is unavailable; it does not retry itself,
and relies on its supervisor's restart policy (`restart: unless-stopped` in
`compose.prod.yaml`) for retry and backoff.

### Reverse proxy and client addresses

The server speaks plain HTTP. A public server runs behind a reverse proxy
that terminates TLS, and the work is split between them:

| Protection | Where | How |
| --- | --- | --- |
| TLS, `Strict-Transport-Security` | proxy | Only the proxy knows the client's scheme |
| Total time to read a request body | proxy | Caddy `read_body`; nginx buffers the whole body before forwarding it |
| Request-head and body-idle timeouts, connection cap | both | The server's `CRISP_HEADER_TIMEOUT_SECS`, `CRISP_BODY_IDLE_TIMEOUT_SECS`, `CRISP_MAX_CONNECTIONS` also hold if the proxy is bypassed or misconfigured |
| Per-operation rate limits (`CRISP_LIMIT_*`) | server | Keyed by the client address below |
| `X-Content-Type-Options: nosniff`, `Cache-Control: no-store`, `Referrer-Policy: no-referrer`, `X-Frame-Options: DENY` | server | On every response, so a downloaded bundle is never rendered as anything but `application/octet-stream` |

**Trusted proxies.** Forwarding headers are written by whoever sends the
request, so by default the server believes none of them: the client is the
TCP peer, and behind a proxy every client shares the proxy's budgets. Set
`CRISP_TRUSTED_PROXIES` to the address the proxy connects from. The server
then reads the header named by `CRISP_PROXY_HEADER` (`X-Forwarded-For` by
default) from the right: the element a trusted peer wrote names the client,
unless that address is itself trusted, in which case the next element to its
left is read. Anything further left was written by the client and is
ignored, so a client cannot choose the address it is limited by. This holds
only while nothing but the proxy can reach the server: keep it bound to
loopback or a private network, as `compose.prod.yaml` does. The startup log
line `client addresses` prints the policy in force.

Under `compose.prod.yaml` the server sees the host proxy as the gateway of
the `crisp_backend` network:

```bash
docker network inspect crisp_backend --format '{{range .IPAM.Config}}{{.Gateway}}{{end}}'
# CRISP_TRUSTED_PROXIES=172.18.0.1   (in /etc/crisp/crisp.env)
```

**Caddy** (automatic certificates; replaces a client's `X-Forwarded-For` with
the address it saw):

```caddy
{
	servers {
		timeouts {
			read_header 10s
			read_body 120s
			idle 2m
		}
	}
}

api.example.com {
	request_body {
		max_size 17MiB
	}
	header Strict-Transport-Security "max-age=31536000; includeSubDomains"
	reverse_proxy 127.0.0.1:8080
}
```

**nginx** (certificates from your ACME client):

```nginx
server {
    listen 443 ssl;
    http2 on;
    server_name api.example.com;
    ssl_certificate     /etc/letsencrypt/live/api.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/api.example.com/privkey.pem;

    client_header_timeout 10s;
    client_body_timeout 30s;
    send_timeout 30s;
    client_max_body_size 17m;
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        # Overwrite, never append: the server reads only the element this
        # proxy writes.
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 60s;
    }
}
```

Both allow a little more than `CRISP_MAX_BUNDLE_BYTES` (16 MiB) so an
oversized upload reaches the server and gets its documented `413`
`application/problem+json` answer. Per-client connection limits and address
bans, where you want them, also belong in the proxy.

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
