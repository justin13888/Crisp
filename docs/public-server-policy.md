# Public server policy: quotas, deletion, and moderation

This document records how `crisp-server` controls what a public deployment
stores, how content and accounts leave it, and how an operator moderates it.
It is a decision record: each section states what the server must do, and the
follow-up issues listed at the end implement it. Where this document and the
code disagree, the code is behind and the matching follow-up issue is open.

It covers storage and moderation policy only. Request-rate budgets belong to
the endpoint rate-limiting work (#42), the reverse-proxy and client-IP model
to the HTTP hardening work (#47), and backups, retention, and platform-key
custody to the production operations work (#48). This document says where
those depend on its decisions.

It is not legal advice. It lists the capabilities an operator needs to meet
typical obligations of a public host (copyright takedown, erasure requests);
whether a given deployment meets its own obligations is the operator's
question.

## Current state

Observed on `master` at `0f3b707`:

- Registration (`POST /auth/register`) is open and unlimited. Any account may
  publish any number of bundles, each up to `CRISP_MAX_BUNDLE_BYTES` (default
  16 MiB, below the spec §12 ceiling of 64 MiB).
- Bundles are stored whole, as the endorsed bytes, in `bundles.raw BYTEA`.
  Every revision of a document is a separate row (`UNIQUE (doc_id, revision)`),
  so every revision stores a full copy of its images.
- No route deletes an account, unpublishes a bundle, or takes content down.
  `bundles.published_by` references `accounts(usr_id)` with no `ON DELETE`
  clause, so an account with bundles cannot be deleted. `recipes_index`
  already cascades from `bundles`.
- There is no administrator, report intake, takedown, or suspension.
- The server endorses every bundle that passes verification. An endorsement
  attests "published on this platform at `signed_at`" (spec §8.4). The spec
  defines no way to revoke one: E2 checks only that the platform key is known
  and that `signed_at` falls in its validity window (§8.5, §11.1).

## 1. Storage model

**Bundles stay in PostgreSQL.** A bundle is one signed file whose bytes must be
served exactly as stored, so the unit of storage is the whole bundle, not its
images: images cannot be split out without re-encoding the file and breaking
its signatures. Moving `bundles.raw` to object storage would add a second
system to keep consistent with the index, and a second thing to back up and
erase from, without changing what a quota has to count. Every database access
already goes through the `Storage` trait (`server/src/storage.rs`), so moving
the bytes later changes one implementation, not the handlers.

Revisit this when either holds: the `bundles` table exceeds roughly 100 GiB,
or backup and restore times exceed what #48's recovery objective allows. The
move then stores `raw` in an S3-compatible bucket keyed by `content_hash`,
keeps every other column in PostgreSQL, and writes the object before the row
so a row never points at a missing object.

**Expected growth.** These are estimates, not measurements; #48's metrics
should replace them. A text-only recipe bundle is tens of kilobytes, most of it
signatures and public keys (a composite signature is 3373 bytes and an author
key set about 2 KB). A bundle with one or two compressed photos is typically
200 KB to 1 MB. Each revision stores the whole bundle again. An active user
publishing ten recipes a month with three revisions each at about 400 KB adds
about 12 MB a month, well inside the default quota below for a year or more.
Search index rows are a few kilobytes each and do not change this picture.

## 2. Quotas

Quotas bound what one account can make the server store. They count stored
bytes and stored bundles across every revision the account published, because
that is what costs disk and backup space.

| Limit | Variable | Default |
|---|---|---|
| Stored bytes per account | `CRISP_QUOTA_BYTES_PER_ACCOUNT` | 256 MiB |
| Stored bundles (revisions) per account | `CRISP_QUOTA_BUNDLES_PER_ACCOUNT` | 5000 |
| Stored bytes across the server | `CRISP_MAX_STORED_BYTES` | unset (no cap) |

- Each value is a positive integer. Following `server/src/config.rs`, a
  malformed value is a startup error, and the per-account byte quota must be
  at least `CRISP_MAX_BUNDLE_BYTES`. That is a floor, not a guarantee: usage
  counts the stored, endorsed bundle, which is larger than the upload by the
  server's endorsement signature, so the quota should sit well above the
  upload limit for a maximum-size upload to fit an empty account.
- `bundles` gains a `size_bytes BIGINT NOT NULL` column (the length of `raw`)
  and an index on `published_by`. Usage is `SUM(size_bytes)` and `COUNT(*)`
  over the account's rows.
- `POST /bundles` checks quota inside the transaction that inserts the bundle,
  after locking the publisher's `accounts` row (`SELECT ... FOR UPDATE`), so
  two concurrent publishes cannot both pass a check only one of them fits.
  Verification runs first, so a rejected bundle never counts.
- Exceeding a per-account limit answers **403** with an RFC 9457 problem of
  type `urn:crisp:problem:quota-exceeded` (`code` `quota_exceeded`), carrying
  `limit`, `used`, and `requested` (bytes or bundles, named by a `quota` field:
  `bytes` or `bundles`). Reaching the server-wide cap answers **507** with type
  `urn:crisp:problem:storage-full` (`code` `storage_full`). Both are added as
  error kinds in `server/src/error.rs`, so the OpenAPI document and the
  generated client describe them.
- `GET /account/usage` (authenticated) returns the account's `bytes_used`,
  `bytes_limit`, `bundles_used`, and `bundles_limit`, so the client can show
  usage before an upload fails. The client renders `quota-exceeded` as a
  message telling the user how much they use and that deleting old revisions
  or documents frees space.
- Freeing space is the publisher's job: the server never prunes old revisions
  on its own, because exact-version retrieval
  (`GET /bundles/{doc_id}/{revision}`) is a published contract.

**Publish rate** is a request budget, not a storage quota, and #42 owns it.
Its recommended starting value is 60 publishes per account per hour: enough
for a user re-sealing a recipe while editing it, and it caps how often one
account can make the server run the full verification pipeline and an
endorsement signature. Registration volume per client address is likewise
#42's, and depends on #47's trusted-proxy model to know the address.

## 3. Deletion

Deletion is **hard**: rows are removed, not flagged. The server keeps no copy
of erased content, so an erasure request is satisfied by the database, with
backups ageing it out on the retention schedule #48 sets.

### 3.1 Unpublishing

- `DELETE /bundles/{doc_id}/{revision}` removes one revision.
  `DELETE /bundles/{doc_id}` removes every revision of a document.
- Only the account in `published_by` may delete its bundles. Another account
  gets **403**; a document or revision that does not exist gets **404**.
- Ownership is per revision, not per document. Any author of a document may
  publish a revision of it (`caller_is_author` in `server/src/bundles.rs`), so
  co-authors can hold revisions of the same `doc_id` under different accounts.
  `DELETE /bundles/{doc_id}/{revision}` on a co-author's revision gets
  **403**. `DELETE /bundles/{doc_id}` removes only the revisions whose
  `published_by` is the caller, and answers **403** when the caller published
  none of the document's revisions. A publisher cannot remove a co-author's
  work.
- Index rows go with the bundle through the existing `ON DELETE CASCADE`.
  Removing the newest revision makes the next newest the one
  `GET /bundles/{doc_id}` returns, whichever account published it.
- Once no revision of a document remains, it answers **404**, as if it had
  never been published, and any of its authors may publish it again. While a
  co-author's revisions remain, the document stays served at the newest of
  them. No tombstone is kept, so nothing about the removed revisions remains
  to erase later.
- Copies already downloaded are not affected: the format is offline-first and
  a downloaded bundle is the reader's file. The client keeps local copies when
  the server no longer has a document, and where it refreshes a document from
  the server it shows that the server no longer offers it.

### 3.2 Account deletion

- `DELETE /account` (authenticated) deletes the account, its sessions and
  login challenges (already `ON DELETE CASCADE`), and every bundle it
  published, in one transaction. `bundles.published_by` gains
  `ON DELETE CASCADE`; the handler logs the number of bundles and bytes
  removed at INFO before committing.
- It requires a **recent login**: the session must be at most 10 minutes old
  (`sessions.created_at`), so a stolen long-lived token cannot erase an
  account. The client asks the user to sign in again with their key file
  before deleting. This reuses the existing login handshake (spec §8.7) and
  needs no new signed message.
- The username becomes available again immediately. Recipes are bound to the
  author's key, not the username, so a new holder of the name cannot pass off
  as the old author's signatures.
- The cascade follows `published_by`, the same per-revision rule as §3.1:
  revisions a co-author published of a document the deleted account also
  authored stay, and the document remains served at the newest of them.
- Others' forks of the deleted account's recipes are their own documents
  (spec §7.5), published by their own accounts, and stay.
- Moderation records survive deletion where §4 says they do: a suspension
  record keeps the suspended `usr_id`, and an audit entry keeps the acting
  administrator's and the target's identifiers. Reports filed by the deleted
  account keep their content with the reporter set to null.

## 4. Moderation

Moderation is **reactive**: the server publishes everything that passes
verification and acts on reports and notices afterwards. Pre-publication
review would make every publish wait on a human, which an offline-first
format whose authoring happens on the client has no use for.

### 4.1 Administrators

- `CRISP_ADMIN_USR_IDS` lists the `usr...` ids of administrator accounts
  (comma-separated, empty by default). An administrator signs in like any
  account; routes under `/admin` require a session whose account is listed.
  No account becomes an administrator through the API.
- There is no administrator interface in the client at first; the operator
  calls the `/admin` routes directly.

### 4.2 Reports and notices

- `POST /reports` (authenticated) files a report against a document, and
  optionally one revision, with a category (`copyright`, `illegal`, `spam`,
  `harassment`, `other`) and up to 2000 characters of detail. An account may
  hold at most 20 open reports, and #42 rate-limits the route.
- Rights holders and authorities without an account use the operator's
  contact address. `GET /server-info` gains optional `abuse_contact` and
  `terms_url` fields, set by `CRISP_ABUSE_CONTACT` and `CRISP_TERMS_URL`; the
  server logs a WARN at startup when either is unset. The client shows the
  terms at registration and offers the abuse contact from the recipe view.
- `GET /admin/reports?status=open` lists reports; `POST
  /admin/reports/{id}/resolve` closes one as `dismissed` or `actioned`.

### 4.3 Takedown

- `POST /admin/takedowns` removes a document, or one revision, regardless of
  who published it, with a reason category and an optional report id.
- Unlike a publisher's removal, a takedown leaves a **tombstone**: the
  `doc_id`, the removed `content_hash` values, the reason category, and the
  time. `GET` on a taken-down document or revision answers **410** with code
  `removed` and the reason category, and `POST /bundles` refuses to publish
  that `doc_id` again with **409** code `removed`. A takedown does not reach
  forks, which are separate documents; each needs its own report and action.
- `DELETE /admin/takedowns/{doc_id}` lifts a takedown (for example after a
  counter-notice) by deleting the tombstone; the author may then publish again.
  Counter-notices themselves are handled by the operator through
  `abuse_contact`, outside the server.

### 4.4 Suspension

- `POST /admin/accounts/{usr_id}/suspend` suspends an account with a reason,
  optionally taking down all its documents in the same action.
  `POST /admin/accounts/{usr_id}/unsuspend` reverses it.
- A suspended account cannot sign in (**403** code `account-suspended`), its
  sessions are deleted at suspension, and registering again with the same key
  set (the same `usr...` id) is refused. Suspension blocks the key, not the
  person: a new key is a new account, and #42's registration budget is what
  bounds that.

### 4.5 Audit

Every administrator action writes an append-only `moderation_actions` row
(actor `usr_id`, action, target, reason, time, report id) and an INFO log
event with the same fields. The actor and target are stored as plain text,
not foreign keys, so the record outlives the accounts it names.

## 5. Endorsements are not revoked

The spec stays as it is. An endorsement attests that the bundle was published
on this platform at `signed_at` (spec §8.4); removing the bundle later does not
make that statement false, so the endorsement stays valid on copies already
downloaded. The server withdraws availability, not the historical attestation.

A revocation list in `GET /server-info` or `GET /platform-keys` was considered
and rejected for v1:

- It cannot reach offline readers, which spec §11.3 promises can verify a
  bundle with no server contact beyond the pinned key set. Copies already
  distributed would keep verifying everywhere that matters to an offline-first
  format.
- It would change the verification procedure (E2 would need an online,
  time-dependent input), which takes the CDDL, `spec/README.md`, `crisp-core`,
  the compliance suite, and the generated declarations together, for a
  guarantee it cannot deliver.
- A platform key that must stop vouching for anything, such as a compromised
  key, is already handled by retiring it with `valid_until` (§8.5), which #48
  covers.

What does change is the wording: spec §8.4 should say explicitly that an
endorsement attests publication at `signed_at` only, not ongoing
availability, approval, or a licence check, and that a server answering 404
or 410 for a document means it no longer offers it. That is a prose
clarification with no effect on the CDDL, verification, or compliance suite.

## 6. Licences and attribution

- Every recipe carries an SPDX licence identifier or `NONE` (spec §13; CDDL
  `license`). The licence is the author's statement. The server checks only
  what the spec checks and does not judge whether the author had the right to
  grant it; a wrong claim is handled by a `copyright` report and a takedown.
- A content licence such as `NONE` does not by itself give the operator the
  right to store, endorse, and redistribute a bundle. The operator's terms of
  service, linked from `terms_url`, must grant that, and the client shows them
  at registration.
- Attribution travels in the bundle: authors and their keys in
  `meta.authors`, and a fork's parent in `derived_from` (spec §7.5). The server
  never rewrites a bundle, so it cannot strip either.

## Follow-up issues

| Issue | Covers |
|---|---|
| #49 | §1 and §2: `size_bytes`, quota enforcement, `GET /account/usage`, client handling |
| #50 | §3: unpublishing, account deletion, client behaviour for removed documents |
| #52 | §4 and §6: administrators, reports, takedowns, suspension, audit, `abuse_contact` and `terms_url` |
| #53 | §5: clarify endorsement semantics in spec §8.4 |
| #42 | Publish rate and registration budgets |
| #47 | Trusted-proxy model for client addresses |
| #48 | Backup retention for erased data; storage metrics replacing §1's estimates |
