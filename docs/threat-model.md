# Threat model: a public crisp-server

This document states who attacks a public Crisp server, what they are after,
and what stops them. It covers `crisp-server` deployed as
[`docs/deployment.md`](deployment.md) describes, the web client in `client/`,
and the protocol between them. The format's own threats (forgery, tampering,
parser attacks on a bundle) are in spec §10; this document relies on them and
does not repeat them.

Where a mitigation names a mechanism, the mechanism is in the code or the spec
today. Residual risks are listed at the end of each section, and the last
section records the security review of `server/` and the client-server
protocol.

## Deployment assumed

The supported topology is one host with a TLS-terminating reverse proxy in
front of the server, which listens on loopback only
([Topology](deployment.md#topology)). The proxy owns TLS, HSTS, and the total
time to read a request body; the server owns its own request-head and
body-idle timeouts, connection cap, and per-operation rate budgets, so they
hold even if the proxy is bypassed or misconfigured. Rate budgets are keyed by
client address only when `CRISP_TRUSTED_PROXIES` names the proxy; otherwise
every client shares the proxy's budget. PostgreSQL has no published port.

## Assets

| Asset | Why it matters |
| --- | --- |
| Platform endorsement key (`CRISP_PLATFORM_SEED`) | Signs every endorsement and every key-set and revocation-list signature; its `usr…` id is the login audience. Whoever holds it speaks as the server. |
| Account sessions | A bearer token publishes, unpublishes, and deletes as its account. |
| Identity keys | A cook's seed signs their recipes and logins. It never reaches the server; the client stores it wrapped in a key file (spec §8.6). |
| Stored bundles and the index | What the server is for; also what quotas bound and backups protect. |
| Reputation of endorsements | An endorsement says "published here at `signed_at`". Endorsing abusive content, or endorsing as someone else, spends that reputation. |
| Availability | Verification and signing are CPU-heavy; the database is shared by every request. |
| Moderation records | The audit trail and tombstones are the evidence for takedowns and suspensions. |

## Adversaries

1. **Anonymous internet client.** Can call every unauthenticated route,
   register any number of accounts (each a free key pair), and send arbitrary
   bytes. Wants to exhaust resources, enumerate accounts, or get content
   endorsed.
2. **Registered user.** Holds a valid session. Wants to publish as someone
   else, exceed quotas, take over other documents, or get abusive or unlawful
   content endorsed.
3. **Network attacker.** Sits between client and proxy. TLS stops them except
   on first contact with a server whose certificate they can forge, and on
   any hop left in clear text.
4. **Malicious or compromised server.** A server the client talks to may lie:
   relay another server's login challenge, offer a different key set, or
   withhold revocations.
5. **Insider or host compromise.** Someone with the host, the env file, a
   backup, or an administrator account.

## Threats and mitigations

### Platform key

| Threat | Mitigation |
| --- | --- |
| Seed read from the host or a backup | Root-only `/etc/crisp/crisp.env`, never in the database, never logged (`Debug` omits it); custody rules in [Platform key custody](deployment.md#platform-key-custody). |
| A leaked key endorses arbitrary bundles, backdated into its window | Compromise runbook: the key leaves the trusted set entirely (spec §8.4); a key set it signs proves nothing, so clients must re-pin by hand. |
| A server swaps its key without the cook noticing | Clients pin the key set per server and accept a change only when a current pinned key signed it (spec §8.5). |
| A retired key that leaked revokes everything | Only a key pinned as current may sign a revocation list (spec §8.8). |

Residual: first contact is trust on first use. A network attacker who can
impersonate the server at that moment pins their own key; TLS is the only
defence there.

### Sessions and login

| Threat | Mitigation |
| --- | --- |
| Login replay | The challenge nonce is consumed atomically before anything else (`DELETE … RETURNING`); a second use fails. |
| Relaying another server's challenge | The login signature binds the server's platform `usr…` id as audience (spec §8.7). |
| Account enumeration through challenges | An unknown username gets a decoy nonce that can never be redeemed. |
| Token theft from the database | Sessions are stored as a digest of the token, never the token. |
| Brute-forcing challenges or logins | `CRISP_LIMIT_CHALLENGE` (per username), `CRISP_LIMIT_CHALLENGE_PEER` and `CRISP_LIMIT_LOGIN` (per client address). |
| Cross-site request forgery | Authentication is a bearer header, never a cookie, so a cross-site page cannot ride a session; `CRISP_ALLOWED_ORIGINS` is mandatory under `compose.prod.yaml`. |
| A suspended account keeps working | Suspension deletes its sessions and challenges in the same transaction; login refuses it after the signature verifies. |

Residual: a token is valid for `CRISP_SESSION_TTL_SECS` (30 days by default)
and is held in the browser's storage, so script running in the client's
origin can read it. The client's Content-Security-Policy admits script only
from that origin, which narrows this to a compromise of the files the origin
serves (see the review findings).

### Publication and endorsement

| Threat | Mitigation |
| --- | --- |
| Publishing as someone else, or taking over a document id | The full A–E verification runs on every upload; E1 requires the document owner's signature, and the caller must be one of the bundle's authors. |
| A crafted bundle exhausts memory or CPU | `CRISP_MAX_BUNDLE_BYTES` caps the body; the spec §12 limits bound the parser; verification runs on a bounded CPU pool (`CRISP_VERIFY_CONCURRENCY`). |
| Double endorsement | An upload already carrying any endorsement is refused. |
| Filling the disk | Per-account byte and bundle quotas and an optional server-wide cap ([policy §2](public-server-policy.md#2-quotas)); `CRISP_LIMIT_PUBLISH` per account. |
| Abusive or unlawful content stays endorsed | Takedown removes the bundles and **revokes their endorsements**; connected clients then fail them with `E.SIG.REVOKED` (spec §8.8). A revoked bundle is never endorsed again, even after the takedown is lifted. |
| Sybil accounts | `CRISP_LIMIT_REGISTER` per client address; suspension refuses the key at registration. |

Residual: revocation reaches only readers that contact the server; copies
held offline keep verifying (spec §8.8, §11.3). The endorsement's time is the
server's clock.

### Availability

| Threat | Mitigation |
| --- | --- |
| Slowloris and idle connections | Request-head and body-idle timeouts and a connection cap in the server, total body time in the proxy. |
| Expensive queries | Browse, search, and the revocation list share `CRISP_LIMIT_QUERY`; page size and offset are clamped. |
| Panics | Recovered into a 500 that carries none of the panic's payload. |
| Database outage | Every database-backed route answers 503, never 500 or 401; `/ready` reports it, `/health` does not. |

Residual: the revocation list is returned whole and signed per request, so
its cost grows with the number of takedowns. It is bounded by the query
budget today; pagination or a cached signature becomes worthwhile once the
list holds thousands of entries.

### Moderation and insiders

| Threat | Mitigation |
| --- | --- |
| Silent administrator action | Every action writes an append-only `moderation_actions` row (a trigger refuses `UPDATE` and `DELETE`) and an INFO log in one transaction. |
| A non-administrator reaches `/admin` | Admin routes require a session whose account is listed in `CRISP_ADMIN_USR_IDS`. |
| Host compromise | Out of scope for the server's own controls; the runbooks cover rotating the platform key and restoring the database. |

## Security review

Review of `server/` and the client-server protocol, at the head of #90.
Each area below was read in the code, not inferred from documentation.

| Area | Result |
| --- | --- |
| Login challenge and session issue (`server/src/auth.rs`, `server/src/storage/sessions.rs`) | Sound: single-use nonce consumed first, audience-bound signature, decoy challenges, digested tokens, suspension checked after the signature. |
| Bearer authentication and admin authorization (`server/src/auth/bearer.rs`, `server/src/config/moderation.rs`) | Sound: a failed lookup is 503 or 500, never a false 401; admin scope comes from configuration only. |
| Upload path (`server/src/bundles.rs`) | Sound: size cap before reading, verification on the CPU pool, author check, pre-endorsed refusal, tombstone and revocation checks before storing. |
| CORS and response headers (`server/src/routes.rs`) | Sound for a bearer-token API; HSTS is the proxy's. |
| Rate limiting behind the proxy (`server/src/limits.rs`) | Sound when `CRISP_TRUSTED_PROXIES` is set; documented as required. |
| Key pinning, rotation, and revocation (`core/src/verify/keyset.rs`, `core/src/verify/revocation.rs`) | Sound; trust on first use is the accepted residual risk. |
| Web client storage of tokens and keys (`client/src/lib/store/`) | **Finding, fixed in #92:** the client sent no Content-Security-Policy, so nothing limited what script could read the token store. It now runs under one that admits script only from its own origin, as a `<meta>` tag in the build and a header with `frame-ancestors 'none'` from the host ([Serving the web client](deployment.md#serving-the-web-client)). |
| Web client use of revocations (`client/src/lib/library.ts`) | **Gap:** the client does not yet fetch or apply revocation lists — #91. |
