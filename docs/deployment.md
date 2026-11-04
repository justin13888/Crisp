# Deploying and operating crisp-server

This guide covers running `crisp-server` in production: the supported
topology, secrets, the platform endorsement key, backups, migrations,
scaling, observability, and upgrades. It ends with runbooks for the incidents
that need one. For local development use `just dev` and the development
[`compose.yaml`](../compose.yaml) instead; neither is safe on a public host.

## Topology

The supported production topology is **one Linux host running Docker Compose
(or Podman Compose)** with [`compose.prod.yaml`](../compose.prod.yaml):

```
Internet ──TLS──▶ reverse proxy (host) ──▶ 127.0.0.1:8080 crisp-server ──▶ postgres
                                           (container)        (container, internal network only)
```

- **One server replica.** See [Scaling](#scaling) for why.
- **PostgreSQL in a container on the same host**, on a private Compose network
  with no published port, its data in the `crisp_postgres_data` volume.
- **TLS terminates in a reverse proxy on the host** (Caddy, nginx, or a cloud
  load balancer). The server listens on loopback only and speaks plain HTTP.
  What the proxy enforces and what the server does, and the reference Caddy
  and nginx configurations, are in the
  [README](../README.md#reverse-proxy-and-client-addresses).

**Managed PostgreSQL** (RDS, Cloud SQL, Neon, …) is a supported variant: drop
the `postgres` service, point `CRISP_DATABASE_URL` at the managed instance
with `sslmode=require`, and use the provider's point-in-time recovery in place
of the [backup procedure](#backups) below. Everything else in this guide
applies unchanged.

## Host prerequisites

- Docker Engine with the Compose plugin, or Podman with `podman-compose`.
- A reverse proxy with automatic certificates, and a DNS name for the API.
- A firewall that admits only 22 (or your management path), 80, and 443.
- Off-host storage for backups (object storage or another machine).
- A secret store for the platform seed (a password manager vault, a cloud
  secret manager, or an offline copy; see [Platform key custody](#platform-key-custody)).

## Secrets and configuration

All production configuration lives in one root-owned env file outside the
repository, read by Compose for variable interpolation. Each container
receives only the values `compose.prod.yaml` hands it: PostgreSQL never sees
the platform seed.

```bash
sudo install -d -m 0700 /etc/crisp
sudo install -m 0600 /dev/null /etc/crisp/crisp.env
sudoedit /etc/crisp/crisp.env
```

```bash
# /etc/crisp/crisp.env — root:root, mode 0600. Never commit this file.
CRISP_POSTGRES_PASSWORD=   # openssl rand -hex 32 (hex keeps the database URL valid)
CRISP_PLATFORM_SEED=       # openssl rand -hex 32, generated once; see below
CRISP_ALLOWED_ORIGINS=https://app.example.com
CRISP_TRUSTED_PROXIES=     # the crisp_backend gateway; see "Client addresses behind the proxy"
# Optional:
# CRISP_IMAGE=registry.example.com/crisp-server:1.2.3
# CRISP_HTTP_PORT=8080
# CRISP_MAX_BUNDLE_BYTES=16777216
# CRISP_VERIFY_CONCURRENCY=4   # default: the container's CPU count
# CRISP_SESSION_TTL_SECS=2592000
# Storage quotas (docs/public-server-policy.md, §2):
# CRISP_QUOTA_BYTES_PER_ACCOUNT=268435456
# CRISP_QUOTA_BUNDLES_PER_ACCOUNT=5000
# CRISP_MAX_STORED_BYTES=      # default: no server-wide cap
# Moderation (docs/public-server-policy.md, §4); a public server sets all three:
# CRISP_ADMIN_USR_IDS=usr1...  # comma-separated administrator accounts
# CRISP_ABUSE_CONTACT=mailto:abuse@example.com
# CRISP_TERMS_URL=https://example.com/terms
# RUST_LOG=info
# CRISP_LOG_FORMAT=text          # or json; see "Observability"
# CRISP_METRICS_PORT=9464        # host loopback port for metrics; see "Observability"
# CRISP_METRICS_BIND_ADDR=off    # disables the metrics listener
# Listener limits (README, "Server configuration"):
# CRISP_HEADER_TIMEOUT_SECS=30
# CRISP_BODY_IDLE_TIMEOUT_SECS=30
# CRISP_MAX_CONNECTIONS=1024
# CRISP_MAX_STREAMS_PER_CONNECTION=100
# Abuse budgets, REQUESTS/SECONDS; see "Rate limits behind the proxy" below:
# CRISP_LIMIT_REGISTER=10/3600
# CRISP_LIMIT_CHALLENGE=10/300
# CRISP_LIMIT_CHALLENGE_PEER=30/60
# CRISP_LIMIT_LOGIN=30/60
# CRISP_LIMIT_PUBLISH=60/3600
# CRISP_LIMIT_QUERY=120/60
```

`compose.prod.yaml` refuses to start without the three required values. The
full variable list is in the [README](../README.md#server-configuration).

Notes:

- `CRISP_ALLOWED_ORIGINS` is required in production. Leaving it empty allows
  any origin, which is a development convenience only.
- Environment variables are visible to anyone who can run `docker inspect`
  on the host. That group is root-equivalent anyway; keep it to operators.
- `CRISP_POSTGRES_PASSWORD` is applied by the `postgres` image only when the
  data volume is first initialised. Changing it later needs
  `ALTER ROLE crisp PASSWORD '…'` inside the database as well as in the file.

### Rate limits behind the proxy

The server budgets its expensive operations and answers `429` with
`Retry-After` when one is spent. Each `CRISP_LIMIT_*` value is
`REQUESTS/SECONDS`; an unset or empty value keeps the default shown above.
`compose.prod.yaml` passes all six through to the server.

| Variable | Default | Keyed by |
| --- | --- | --- |
| `CRISP_LIMIT_REGISTER` | `10/3600` | client IP |
| `CRISP_LIMIT_CHALLENGE` | `10/300` | submitted username |
| `CRISP_LIMIT_CHALLENGE_PEER` | `30/60` | client IP |
| `CRISP_LIMIT_LOGIN` | `30/60` | client IP |
| `CRISP_LIMIT_PUBLISH` | `60/3600` | authenticated account |
| `CRISP_LIMIT_QUERY` | `120/60` | client IP (browse and search share it) |

The per-username challenge budget and the per-account publish budget are
unaffected by the proxy and can usually keep their defaults. Counters live in
memory, so a restart resets them.

### Client addresses behind the proxy

The server sees the host's reverse proxy connect from the gateway of the
`crisp_backend` network, never from the client. Name that address in
`CRISP_TRUSTED_PROXIES` so the client-IP budgets count the client the proxy
forwards in `X-Forwarded-For`:

```bash
docker network inspect crisp_backend --format '{{range .IPAM.Config}}{{.Gateway}}{{end}}'
```

Check the `client addresses` line in the startup log, which prints the policy
in force. The proxy must overwrite the header rather than pass a client's
through (Caddy does by default; the README's nginx configuration sets it), and
the full trust model is in the
[README](../README.md#reverse-proxy-and-client-addresses).

Left unset, nobody is trusted: every client-IP budget is one limit shared by
all clients, so with the defaults the whole server accepts 10 registrations an
hour and 30 sign-ins a minute, and a single client can spend them for
everyone. If you cannot trust the proxy's address (a managed load balancer
whose address changes), raise the four client-IP budgets to the server-wide
rate you expect at peak instead, for example:

```bash
CRISP_LIMIT_REGISTER=200/3600
CRISP_LIMIT_CHALLENGE_PEER=600/60
CRISP_LIMIT_LOGIN=600/60
CRISP_LIMIT_QUERY=3000/60
```

Raised this way they bound total load on the server, not any one client;
per-client limits then belong in the reverse proxy.

## First deployment

```bash
git clone https://github.com/justin13888/Crisp.git /opt/crisp && cd /opt/crisp
git checkout <release tag or commit>
docker compose -f compose.prod.yaml --env-file /etc/crisp/crisp.env up -d --build
docker compose -f compose.prod.yaml --env-file /etc/crisp/crisp.env ps     # both healthy
curl -fsS http://127.0.0.1:8080/api/v1/server-info
```

Record the platform key's `usr_id` from `server-info` next to the seed in your
secret store: it is how you later confirm a restored seed is the right one.

Then point the reverse proxy at `127.0.0.1:8080`, using the Caddy or nginx
configuration in the [README](../README.md#reverse-proxy-and-client-addresses),
and set `CRISP_TRUSTED_PROXIES` as described in
[Client addresses behind the proxy](#client-addresses-behind-the-proxy).

The image carries a `HEALTHCHECK` against `GET /api/v1/health`, a liveness
probe that does not touch the database (a readiness probe is issue #46).
`compose.prod.yaml` repeats it, because Podman builds OCI-format images by
default and that format drops `HEALTHCHECK` (`podman build --format docker`
keeps it).

## Platform key custody

`CRISP_PLATFORM_SEED` is the 32-byte seed from which the server derives its
Composite ML-DSA endorsement key ([`server/src/platform.rs`](../server/src/platform.rs)).
Every published bundle carries an endorsement signed with it, clients pin its
public key set per server on first contact (spec §8.5), and its `usr…` id is
the audience every login signature is bound to (spec §8.7). Treat the seed as
the server's identity.

**Generate** it once with a CSPRNG on a trusted machine:
`openssl rand -hex 32`. Never derive it from a password, never reuse one across
servers, and never copy one from an example or a development `.env`.

**Store** it in exactly two places:

1. the host's `/etc/crisp/crisp.env` (root-only, mode 0600), and
2. a secret store off the host — a cloud secret manager or password-manager
   vault with access limited to the operators who can deploy, or a printed
   copy in a safe.

Never put it in a repository, a CI variable that untrusted branches can read,
a chat, a ticket, a log, or a database backup. The server never logs it: its
`Debug` output omits the secret half.

**Back it up** separately from the database. A database backup without the
seed restores a server that cannot sign as itself; a seed without a database
backup is still the server's identity. Check the off-host copy after every
change by deriving the `usr_id` (start a throwaway server with it, or compare
with `server-info`) and matching it against the recorded value.

**Access**: only the people who can deploy the server can read the seed.
Rotate their access, not the seed, when someone leaves, unless they could have
copied it (then follow [Platform key compromise](#runbook-platform-key-compromise)).

### Rotation policy

The server holds **one live key** with an open validity window: `valid_from`
and `valid_until` are both unset in `server-info`. Rotating the key today
means replacing the seed, and has these consequences:

- Clients that pinned the old key set see a **key mismatch** warning on their
  next connection and must choose to trust the new set
  (`client/src/lib/servers.tsx`).
- Once a client trusts the new set, it stops trusting the old key, so bundles
  endorsed by the old key fail E2 with `E.KEY.UNKNOWN` on that client.
- Login challenges bind the new `usr…` id; existing sessions keep working
  because a session token does not depend on the key.

Spec §8.5 already allows a key set of current plus retired keys, each with a
`valid_from` and `valid_until`. The server cannot yet publish a retired key,
and neither the spec nor the client says how a pinned set may grow without a
mismatch, so a graceful rotation needs changes in all three; that work is
issue #57.

Until then the policy is: **do not rotate on a schedule. Rotate only when the
seed is compromised**, and follow the runbook below.

## Backups

Bundles are stored inline in PostgreSQL (`BYTEA`), so the database is the
whole server state and its backup size grows with published content (up to
`CRISP_MAX_BUNDLE_BYTES` per revision). The platform seed is backed up
separately (above).

Take a logical backup daily with `pg_dump`'s custom format, which is
compressed and restores selectively:

```bash
#!/usr/bin/env bash
# /usr/local/sbin/crisp-backup — run daily from cron or a systemd timer.
set -euo pipefail
cd /opt/crisp
stamp=$(date -u +%Y%m%dT%H%M%SZ)
out=/var/backups/crisp/crisp-$stamp.dump
install -d -m 0700 /var/backups/crisp
docker compose -f compose.prod.yaml --env-file /etc/crisp/crisp.env \
  exec -T postgres pg_dump -U crisp -d crisp --format=custom > "$out.partial"
mv "$out.partial" "$out"
# Ship off-host (encrypted at rest there), then prune local copies.
# rclone copy "$out" remote:crisp-backups/
find /var/backups/crisp -name 'crisp-*.dump' -mtime +7 -delete
```

Retention: keep 7 daily, 4 weekly, and 6 monthly backups off-host. Encrypt
them at rest and restrict who can read them: they hold every account's
public keys, every live session token (stored in plain text; hashing them is
issue #44), and every published bundle. Anyone holding a recent backup can
act as any signed-in user until those sessions expire.

`pg_dump` takes a consistent snapshot without stopping the server. For
recovery points finer than a day, use managed PostgreSQL's point-in-time
recovery, or WAL archiving (pgBackRest, WAL-G) on the host.

**Test restores** at least quarterly, and after every PostgreSQL major
upgrade, by running [Database restore](#runbook-database-restore) into a
scratch Compose project and checking `server-info` and a known bundle there.
On the production host, the scratch project must not collide with
production:

- pass `-p crisp-restore-test` to **every** `docker compose` command of the
  runbook, so the scratch stack gets its own containers, network, and
  volume (without it, the commands act on production);
- give it a copy of the env file with a different `CRISP_HTTP_PORT` (e.g.
  `18080`), because the server binds `127.0.0.1:${CRISP_HTTP_PORT}` and
  production already holds the default `8080`;
- start its database first (`... -p crisp-restore-test up -d postgres`),
  since a scratch project has no stack to stop, and remove it afterwards
  with `... -p crisp-restore-test down -v`.

## Migrations

The schema is created and upgraded by SeaORM migrations
([`server/src/db.rs`](../server/src/db.rs)), which the server applies at boot
before it serves anything ([`server/src/lib.rs`](../server/src/lib.rs)).
Startup logs `connected to PostgreSQL; applying migrations`, and a failed
migration exits the process before it binds.

The stance:

- **Migrations run at boot.** With one replica there is no race, and a
  release cannot run against a schema it does not expect.
- **Migrations are forward-only in production.** A migration's `down` exists
  for development; production rolls back by restoring the pre-upgrade backup
  ([Bad migration](#runbook-bad-migration)).
- **Back up before every upgrade** that ships a migration. Release notes say
  when one does; `git diff <old>..<new> -- server/src/db.rs` shows it.
- **Prefer additive migrations** (new tables, nullable columns) so the
  previous release keeps working against the new schema, which keeps a
  rollback of the binary alone possible.

## Scaling

Run **one server replica**. A single instance is the supported configuration
because:

- every replica applies migrations at boot, and SeaORM takes no lock, so
  replicas starting together race on the schema;
- every replica runs the expired-session purge task. That alone is harmless
  (the purge is an idempotent `DELETE`), but it is wasted work.

Scale vertically first: verification and endorsement are CPU-bound, so more
cores and memory on the one host go furthest. Horizontal scaling needs a
separate migration step (or an advisory lock around it) and is out of scope
for this topology.

## Observability

**Logs.** The server writes human-readable `tracing` lines to stdout. Every
request is traced at INFO and correlated with a server-generated request id,
returned to the client in the `x-request-id` header, which ties a client
report to the server's lines. `RUST_LOG` sets the filter, e.g.
`RUST_LOG=info,crisp_server=debug`. Set `CRISP_LOG_FORMAT=json` to write one
JSON object per event instead (its fields, level, target, and current span),
so a log store can query `request_id`, `operation_id`, `status`, or `usr_id`
directly. Compose's `json-file` driver keeps 5 files
of 10 MB per container (`compose.prod.yaml`); ship them to your log store with
the Docker logging driver of your choice if you need longer retention.

```bash
docker compose -f compose.prod.yaml --env-file /etc/crisp/crisp.env logs -f server
```

**Health.** `GET /api/v1/health` is a liveness probe, used by the image's
`HEALTHCHECK`. `docker compose ps` shows the result.

**Metrics.** The server serves Prometheus metrics (OpenMetrics text) at
`GET /metrics` on a listener of its own, `CRISP_METRICS_BIND_ADDR`
(default `127.0.0.1:9464`; `off` disables it). It is never served on the
public listener, so the reverse proxy cannot expose it and it is not part of
`spec/openapi.json`. The listener has no authentication: bind it only to an
address your scraper alone can reach. `compose.prod.yaml` binds it on all of
the container's interfaces and publishes it on host loopback only, as
`127.0.0.1:${CRISP_METRICS_PORT:-9464}`, so a Prometheus (or agent) on the
host scrapes `http://127.0.0.1:9464/metrics`. Never publish the port on a
public interface or route it through the reverse proxy. The series, all
prefixed `crisp_`:

| Metric | Type | Labels | Meaning |
| --- | --- | --- | --- |
| `http_requests_total` | counter | `operation`, `status` | Responses sent, by OpenAPI `operationId` (`<unmatched>` for an unknown path) and status code |
| `http_request_duration_seconds` | histogram | `operation` | Time to produce the response head |
| `cpu_job_duration_seconds` | histogram | `job` | Signature verification and endorsement time on the bounded pool: `publish` (verify and endorse a bundle), `register`, `login` |
| `cpu_job_wait_seconds` | histogram | `job` | Time a job waited for a slot (`CRISP_VERIFY_CONCURRENCY`) |
| `db_pool_connections`, `db_pool_idle_connections`, `db_pool_max_connections` | gauge | | PostgreSQL pool occupancy, sampled at each scrape |
| `purge_runs_total` | counter | `outcome` (`ok`, `error`) | Background purge passes |
| `purged_rows_total` | counter | `kind` (`session`, `challenge`) | Expired rows the purge deleted |

Starting points for alerts:

```promql
# Error rate: share of responses that are 5xx, per operation.
sum by (operation) (rate(crisp_http_requests_total{status=~"5.."}[5m]))
  / sum by (operation) (rate(crisp_http_requests_total[5m]))
# p95 latency per operation.
histogram_quantile(0.95, sum by (operation, le) (rate(crisp_http_request_duration_seconds_bucket[5m])))
# Verification saturating its pool: uploads waiting for a slot.
histogram_quantile(0.95, sum by (le) (rate(crisp_cpu_job_wait_seconds_bucket{job="publish"}[5m])))
# Pool exhausted: every connection in use.
crisp_db_pool_connections - crisp_db_pool_idle_connections >= crisp_db_pool_max_connections
# The purge is failing.
increase(crisp_purge_runs_total{outcome="error"}[1h]) > 0
```

Alert also on the health check and on host CPU, memory, and disk (the
PostgreSQL volume grows with content).

## Upgrades

**crisp-server:**

1. Read the release's changes; note any migration.
2. Take a backup ([Backups](#backups)).
3. `git checkout <new tag>`, then rebuild on fresh base images and restart:

   ```bash
   docker compose -f compose.prod.yaml --env-file /etc/crisp/crisp.env build --pull server
   docker compose -f compose.prod.yaml --env-file /etc/crisp/crisp.env pull postgres
   docker compose -f compose.prod.yaml --env-file /etc/crisp/crisp.env up -d
   ```

   `build --pull` picks up base-image security fixes, so do this at least
   monthly even without a release.
4. Watch the logs for the migration and `platform endorsement key ready`, and
   check that the `usr_id` in `server-info` is unchanged.

**PostgreSQL:** minor versions arrive with `pull postgres` under the
`postgres:17-alpine` tag. A major version cannot reuse the data directory:
back up, change the tag, start with an empty volume, and restore
([Database restore](#runbook-database-restore)).

## Runbooks

### Runbook: platform key compromise

The seed has leaked, or someone who could have copied it is no longer trusted.
An attacker holding it can endorse arbitrary bundles as this server and accept
login signatures bound to its id.

1. Generate a new seed (`openssl rand -hex 32`) and replace it in
   `/etc/crisp/crisp.env` and in the off-host secret store. Keep the old seed
   offline for the investigation; do not delete it yet.
2. Restart: `docker compose -f compose.prod.yaml --env-file /etc/crisp/crisp.env up -d server`.
3. Record the new `usr_id` from `server-info`.
4. Announce the rotation through a channel users already trust, with the old
   and new `usr_id`, the time window of the compromise, and the instruction to
   accept the new key only if it matches the announced id. Clients will show a
   key mismatch until they do.
5. Bundles endorsed by the old key before the compromise remain authentic, but
   clients that trust the new set stop verifying their endorsement (see
   [Rotation policy](#rotation-policy)). Users can republish them to get a new
   endorsement; the author signature is unaffected.
6. Review the access that exposed the seed, and record the incident.

### Runbook: platform key loss

The seed is gone from the host and from the secret store. Nothing can recover
it: the server must take a new key, with the same client-side consequences as a
rotation. Follow steps 1–5 of the compromise runbook, announcing a loss rather
than a compromise. Then fix the custody gap that allowed it.

### Runbook: database restore

Restore from a `pg_dump --format=custom` file after data loss or corruption,
or into a scratch project to test a backup.

1. Stop the server so nothing writes during the restore:
   `docker compose -f compose.prod.yaml --env-file /etc/crisp/crisp.env stop server`
2. Copy the chosen dump to the host, and verify it lists:
   `pg_restore --list crisp-<stamp>.dump | head` (run it in the postgres
   container if the host has no client).
3. Replace the database contents with the dump:

   ```bash
   docker compose -f compose.prod.yaml --env-file /etc/crisp/crisp.env \
     exec -T postgres pg_restore -U crisp -d crisp --clean --if-exists \
       --no-owner --exit-on-error < crisp-<stamp>.dump
   ```

4. Start the server: `... up -d server`. It applies any migration newer than
   the dump at boot.
5. Check `docker compose ps` (healthy), `server-info` (the `usr_id` you
   recorded), and that a recently published bundle downloads.
6. Everything published after the dump's timestamp is lost from the server;
   authors still hold their signed files and can republish them.

### Runbook: bad migration

A release's migration failed at boot, or succeeded but broke the service.

1. **Migration failed at boot:** the server exits before serving and Compose
   restarts it in a loop. On PostgreSQL, SeaORM runs the whole batch of
   pending migrations in one transaction, so a failure rolls back every
   migration of that boot, earlier ones of the same release included, and
   leaves the schema as the previous release left it. Roll back the binary: `git checkout <previous tag>` and `up -d --build server`.
   If the old release fails to start against the schema, continue with step 2.
2. **Migration applied but the release is broken:** stop the server, restore
   the backup taken before the upgrade ([Database restore](#runbook-database-restore)),
   check out the previous tag, and start it. Publications since the backup are
   lost from the server, as in a restore.
3. File an issue with the server's logs from the failed boot, and do not retry
   the upgrade until a fixed release exists.
