# Crisp — Recipe Bundle Format Specification

**Version 1.0.0 (Final).** The structure is frozen: bundles written to this
version will be readable by every future 1.x reader (§14).
A self-contained, signed, non-executable, localized interchange format for
recipes and recipe books.

| | |
|---|---|
| Format name | **Crisp** |
| Unit of distribution | a **Crisp bundle** (one file = one recipe *or* one book) |
| File extension | `.crisp` |
| Media type | `application/crisp+cbor` |
| Debug projection | `application/crisp+json` (non-normative, never signed) |
| Normative schema | `crisp-v1.cddl` (CDDL, RFC 8610) — accompanies this document |

The key words MUST, MUST NOT, SHOULD, SHOULD NOT, MAY are used as in RFC 2119.
This document defines **semantics, algorithms, and verification**; the
**structure** of a bundle is defined normatively by `crisp-v1.cddl`.

---

## 0. Final decisions

These decisions are fixed for v1.

| # | Decision | Rationale |
|---|---|---|
| D1 | **Name is "Crisp."** File = "Crisp bundle", extension `.crisp`, media type `application/crisp+cbor`, CBOR application tag `1129468752`, domain separators `crisp/...`. | — |
| D2 | **Container: canonical CBOR**, RFC 8949 §4.2 deterministic encoding. Single file, fully self-contained — **no external references of any kind**. | Reproducible signatures; no fetch/path-traversal surface. |
| D3 | **Structural schema lives in `crisp-v1.cddl`** (RFC 8610). This document does not restate field shapes in prose. | One authoritative, machine-checkable schema. |
| D4 | **Hash: SHA-256 everywhere** — IDs, revision hashes, blob IDs, content hash, key fingerprints. No alternates. (The composite signature's internal pre-hash is SHA-512, fixed by the IETF construction.) | No compelling reason to diverge; one primitive is simpler to audit. |
| D5 | **Signing: Composite ML-DSA `id-MLDSA65-Ed25519-SHA512`**, exactly as specified by `draft-ietf-lamps-pq-composite-sigs-19` and checked against its Appendix E test vector. One algorithm; no menu. | The emerging PKI standard for hybrid signatures; NIST category 3; pragmatic sizes. See §8. |
| D6 | **A *published* bundle MUST carry ≥1 author signature — including the document owner's — and exactly one server endorsement, last.** Zero signatures = an unpublished draft. | Makes "compliant" externally checkable and unambiguous. |
| D7 | **Links: `https` scheme only.** Cleartext `http`, `mailto`, `data:`, `javascript:`, etc. are rejected. Clients MUST validate. | Strict, minimal, no downgrade or non-web surface. |
| D8 | **Media: raster images only** — `image/png`, `image/jpeg`, `image/webp`, `image/avif`. No SVG, video, audio, or fonts. | SVG is executable; the rest add parser surface for no v1 benefit. |
| D9 | **One profile.** There is a single set of limits (§12). | Fewer modes, fewer divergent implementations. |
| D10 | **Localization is first-class from v1.** Bundles declare `default_language` + `languages`; every human-readable field is a `localized-text`/`localized-prose` map. | Recipes are global; bolting on translations later is worse. |
| D11 | **Identity is frozen at birth**; the cryptographic ID is derived from `{author_fpr, original title, title language, created_at, birth_nonce}` and is **stable across all edits**. Title normalization is NFC + trim + whitespace-collapse, **case-preserving**. | Rename semantics, random birth nonce, no case-folding. |
| D12 | **`created_at` and `modified_at` are mandatory and cryptographically bound** (§9). | Tamper-evident provenance and edit tracking. |
| D13 | **Identities are random keys, not passwords.** An identity is a CSPRNG seed; a passphrase only wraps it in a key file (§8.6). | Public keys appear in every bundle; a passphrase-derived key would be brute-forceable offline. |
| D14 | **Built for cooking under time pressure.** Steps carry multiple labelled timers (active or passive), heat levels, temperature ranges with targets, and per-step ingredient portions; ingredients carry equivalent measures and a never-scale flag. | A cook following a recipe needs these at a glance, not buried in prose. |

---

## 1. At a glance

A Crisp bundle is one file that holds either a single recipe or a whole book,
with all text, images, structure, and signatures inside it.

```
.crisp file
├─ outer wrapper (tiny, unsigned) ──── magic + container version
└─ COSE-style signed envelope
   ├─ protected header (signed) ────── format version, doc id, content hash
   ├─ payload (signed) ─────────────── metadata · recipes[] · book structure
   │                                   · content-addressed blob store
   └─ signatures[] ─────────────────── author signature(s) + server endorsement
                                       (each: Composite ML-DSA, MLDSA65-Ed25519)
```

It is **non-executable by construction** — no scripts, macros, formula or
expression language, stylesheets, fonts, SVG, or markup strings — and **bounded
by construction** — every count, size, and depth has a hard limit, so a
conformant parser is single-pass, total, and DoS-resistant.

---

## 2. Design principles

| Principle | How the format delivers it |
|---|---|
| One self-contained file | Everything inline; no external references at all (D2). |
| Determinism | Canonical CBOR; one input → one byte string → one signature → one ID. |
| Closed core schema | `crisp-v1.cddl` is exhaustive; unknown core fields are rejected; only namespaced `ext` maps are ignorable. |
| No execution surface | No scripting/macros/formulas/SVG/HTML/CSS/fonts. Rich text is pre-parsed structure. Scaling and unit conversion are plain client arithmetic. |
| Bounded everything | Hard limits (§12); non-recursive data model; indefinite-length CBOR rejected. |
| Untrusted by default | Every field is attacker-controlled input until verified (§11). Signatures attest origin and integrity; they do not make content safe to render. |
| Stable identity, mutable body | ID frozen at birth (§7); a separate revision hash tracks exact versions. |
| Externally verifiable | A complete, deterministic verification procedure with stable error codes and a published compliance suite (§11). |

**Why CBOR.** It has a published deterministic encoding (essential for
reproducible signatures), embeds image bytes natively, and underpins COSE
(RFC 9052), a real signing standard. JSON has no canonical form and lossy
numbers; XML carries entity-expansion attack surface; ZIP containers invite
zip-bomb and path-traversal classes.

---

## 3. Container & encoding

### 3.1 Outer wrapper

A `.crisp` file is a CBOR document: self-describe tag `55799` (RFC 8949 §3.4.6)
wrapping the Crisp application tag `1129468752` (`"CRSP"`; a placeholder — a real
tag MUST be registered with IANA before production), wrapping a 2-element array
`[container_version, signed_envelope]`. See `crisp-file` in the CDDL.

Only the magic, the container version, and the COSE framing are unsigned;
everything meaningful is inside the signed envelope.

### 3.2 Deterministic encoding (normative)

All CBOR in a bundle MUST use **CBOR Core Deterministic Encoding** (RFC 8949
§4.2.1):

- definite-length items only — indefinite-length items MUST be rejected;
- integers and lengths in shortest form;
- map keys sorted by encoded bytes; duplicate keys MUST be rejected;
- no floating-point in any field — fractional values use `rational` (CDDL);
- text strings are well-formed UTF-8, normalized to **Unicode NFC**;
- timestamps are integer epoch-seconds, UTC;
- only the two tags above are permitted; any other tag MUST be rejected.

This is what makes signatures reproducible and closes the parser-differential
class of signature-bypass bugs.

---

## 4. Data model

The complete structure is `crisp-v1.cddl`. This section gives the semantics the
schema cannot express.

- **`bundle-payload`** — the signed content. `kind` is `recipe` or `book`.
  `recipes[]` is a flat list; a book references entries by index, so a recipe
  reused across chapters is stored once.
- **`recipe`** — carries a frozen `identity`, a derived `id`, a `revision` hash,
  `modified_at`, localized display fields, structured `ingredients`/`steps`, and
  classification (`course`/`diet`/`allergens` as language-neutral codes).
- **`identity`** — the five frozen birth fields that feed the ID (§7). For a
  `book`, the bundle's identity lives in `meta.identity`.
- **`quantity` / `rational`** — amounts are exact rationals, never floats.
  Scaling a recipe is multiplication of every `rational`; there is deliberately
  no embedded formula language.
- **`prose` / `run`** — "rich text" is a depth-2, non-nesting structure
  (`prose` → `paragraph` → `run`). There is no markup string and therefore no
  markup parser to attack. `run.link` is `https`-only; `run.ref` is an internal
  pointer that never leaves the bundle.
- **`blob`** — a raster image, content-addressed: `blob-id =
  bech32m("blb", 0x01 ‖ SHA-256(bytes))`. Reused images are stored once.
- **`book-structure`** — a fixed-depth tree (book → part → chapter → entry); no
  recursion, no cycles.

**Internal references** (`step.ingredients[].ref`, `step.equipment`, `group`
indices, `recipe-entry.recipe`, `run-ref.target`) are bounded handles; each
`ref` is unique within its list and every reference MUST resolve inside the
bundle. `derived_from`, `supersedes`, and `ingredient.recipe` are ids that
are checked for form only. Resolving any of them never requires a network or
recursion.

**Scaling.** Scaling multiplies the yield, every ingredient `quantity` and
`alt`, substitute quantities, and step portions by an exact rational — except
for `fixed` ingredients. Timers, temperatures, equipment, and nutrition never
scale.

---

## 5. Localization (first-class)

Localization is built into the data model, not an extension.

- The bundle declares **`default_language`** and a **`languages`** list (BCP 47).
- Every human-readable field is a **`localized-text`** or **`localized-prose`**
  map: `{ BCP-47-tag => value }`.
- Language-**neutral** data is *not* localized: all `ref` handles, IDs,
  timestamps, `rational` quantities, UCUM units, temperatures, durations, blob
  references, and the `course`/`diet`/`allergen`/`nutrient` code vocabularies.
- A recipe's structural skeleton (the ingredient list with quantities, the step
  sequence with refs/durations/temperatures) is shared across all locales; only
  names and prose differ per locale.

**Rules (verifier-enforced):**

1. Every `localized-text`/`localized-prose` map MUST contain the bundle's
   `default_language` key.
2. Every key of every localized map MUST appear in `languages`.
3. `default_language` MUST appear in `languages`.
4. `identity.language` (the language of the frozen identity title) MUST appear
   in `languages`.

A client renders the user's preferred language if present, else
`default_language`.

---

## 6. *(reserved)*

---

## 7. Identity & the ID system

A recipe's ID is derived **only from stable birth-identity fields**, so it
survives every edit to the recipe body, while still cryptographically
distinguishing genuinely different recipes.

### 7.1 The identity tuple

`identity` (CDDL) has exactly five frozen fields:

| Field | Meaning |
|---|---|
| `author_fpr` | The author's key fingerprint: `SHA-256(canonical_cbor(["crisp/user-id/1", public-key-set]))` (32 bytes). |
| `title` | The **original** title, NFC + trimmed + internal whitespace collapsed to one ASCII space. Frozen; **case-preserving**. The *display* title may be edited and localized freely. |
| `language` | BCP 47 language of `title`. |
| `created_at` | Creation instant, epoch seconds. Frozen. |
| `birth_nonce` | 128-bit CSPRNG value, fixed at birth. |

The `birth_nonce` is what makes collision-avoidance real: hashing
`(author, title)` alone cannot avoid collisions, because identical inputs
produce identical IDs *by definition*. The nonce guarantees two genuinely
distinct recipes — even same author, same title — get distinct IDs, and being
fixed at birth it is exactly "metadata that does not change."

### 7.2 Derivation

```
domain   = "crisp/recipe-id/1"        ; "crisp/book-id/1" for a book
preimage = canonical_cbor([
             domain,
             identity.author_fpr,                 ; bstr(32)
             normalize(identity.title),           ; NFC, trim, ws-collapse
             identity.language,                   ; BCP-47 tstr
             identity.created_at,                 ; uint
             identity.birth_nonce                 ; bstr(16)
           ])
digest   = SHA-256(preimage)                      ; 32 bytes
id       = bech32m( hrp, 0x01 || digest )
```

- **Hash:** SHA-256 (D4). 256-bit output → ~128-bit classical collision
  resistance, the standard target.
- **Domain separation** ensures a recipe ID, a book ID, a user ID, a revision
  hash, and a blob ID can never collide even with identical inputs.
- **Encoding:** `bech32m` — case-insensitive, no ambiguous characters, with a
  6-character checksum that detects copy-paste typos and truncation. Every Crisp
  ID is a fixed 63-character string: a 3-char HRP, `1`, then 59 data/checksum
  characters.

| Object | HRP | Domain separator |
|---|---|---|
| Recipe ID | `rcp` | `crisp/recipe-id/1` |
| Book ID | `rbk` | `crisp/book-id/1` |
| User ID | `usr` | `crisp/user-id/1` |
| Revision hash | `rev` | `crisp/revision/1` |
| Blob ID | `blb` | *(no domain string — `SHA-256(bytes)` directly)* |

### 7.3 Revision hash — exact-version identity

The ID answers "is this *the same recipe*?" The revision hash answers "is this
the *exact same version*?":

```
revision = bech32m("rev", 0x01 || SHA-256(
             canonical_cbor( recipe without the "revision" key )))
```

`modified_at` is **kept inside** the revision hash, so every save is a distinct,
attestable revision. `supersedes` links each revision to its predecessor,
forming a hash-linked (Merkle) edit history. `(id, revision)` together name one
precise version.

### 7.4 Author binding

One 32-byte **key fingerprint**, `SHA-256(canonical_cbor(["crisp/user-id/1",
keyset]))`, names a key everywhere: it is a recipe's `author_fpr`, a
signature's `kid`, and the digest inside the key's `usr…` User ID
(`usr… = bech32m("usr", 0x01 || fingerprint)`). A recipe's ID is therefore
permanently bound to a key the author controls;
the author signature (§8) proves that control. A loaded recipe MUST have its
`id` recomputed from `identity` and checked — you cannot claim an ID that does
not derive from your own identity fields.

### 7.5 Forks & key rotation

A fork sets `derived_from` to the parent ID and gets its own fresh `identity`
(new nonce → new ID). A user who rotates keys gets a new User ID; *future*
recipes are born under it, while *existing* recipe IDs stay valid and bound to
the old key — identity is stable by design. Cross-key continuity (an old key
signing a statement endorsing a new key) is an ecosystem concern outside the
file format.

---

## 8. Signing — Composite ML-DSA

### 8.1 The algorithm (final decision)

Crisp v1 mandates exactly one signature algorithm: **Composite ML-DSA
`id-MLDSA65-Ed25519-SHA512`**, exactly as specified by the IETF LAMPS draft
`draft-ietf-lamps-pq-composite-sigs-19`, and verified against that draft's
Appendix E test vector:

```
M'       = "CompositeAlgorithmSignatures2025" || "COMPSIG-MLDSA65-Ed25519-SHA512"
           || len(ctx) || ctx || SHA-512(M)          ; ctx is empty in Crisp
mldsaSig = ML-DSA-65.Sign(mldsaSK, M', ctx = "COMPSIG-MLDSA65-Ed25519-SHA512")
tradSig  = Ed25519.Sign(edSK, M')
sig      = mldsaSig || tradSig                       ; 3309 + 64 = 3373 bytes
```

ML-DSA signing SHOULD be hedged (randomized), as FIPS 204 recommends. The
public key set is the `public-key-set` map. Crisp's application context is
empty because every message Crisp signs is already domain-separated by a
`crisp/…` string inside `M`.

It is a true hybrid: one signing operation produces a composite signature whose
verification **requires both** an **ML-DSA-65** (FIPS 204, NIST category 3,
lattice) component **and** an **Ed25519** (RFC 8032) component to verify. An
attacker must break *both* primitives to forge — so the bundle stays secure if
either is later broken, including by a quantum adversary.

It was chosen over the alternatives because it is the *standardized* hybrid the
broader PKI ecosystem is converging on (rather than a bespoke
sign-twice-and-concatenate scheme), the composite specification itself handles
the inter-component domain separation and non-separability, ML-DSA-65 is the
recommended general-purpose parameter set (signature ≈ 3.3 KB; public key
1952 B), and Ed25519 is small, fast, and universally available. A future major
version may escalate to `MLDSA87-Ed448`; v1 does not offer it as an option.

Both **author** identity keys and the **platform** signing key are
`MLDSA65-Ed25519` composite keys, represented as the `public-key-set` map.

### 8.2 Signature envelope

Signatures follow the shape of `COSE_Sign` (RFC 9052) — see `cose-sign` in the
CDDL. The `protected-header` binds `{format version, doc_id, content_hash}`
where `content_hash = SHA-256(payload)`. Each `signature` carries a
`sig-protected` map declaring `alg`, `role` (`author` or `server-endorsement`),
`kid` (the signer's key fingerprint, §7.4), and `signed_at`; an endorsement also
carries `binds` (§8.3).

### 8.3 What is signed

Each signature is computed over a domain-separated structure, never over raw
payload bytes:

```
M = canonical_cbor([
      "crisp/signature/1",
      protected_header_bstr,     ; binds format version, doc_id, content_hash
      sig_protected_bstr,        ; binds alg, role, kid, signed_at
      external_aad               ; see below
    ])
sig-value = CompositeMLDSA.Sign(signer_secret_key, M)
```

- For an **author** signature, `external_aad` is empty (`h''`).
- For a **server endorsement**, `external_aad` and the endorsement's
  `sig-protected.binds` are both
  `SHA-256(canonical_cbor([author_signature_1, …, author_signature_n]))`,
  binding the endorsement to every author signature, in order — a verifiable
  chain of attestations. Carrying `binds` explicitly lets a verifier tell a
  mis-bound endorsement (`E.SIG.BINDING`) from a forged one
  (`E.SIG.ENDORSEMENT`).

Because `sig_protected` (covered by the signature) names `alg: MLDSA65-Ed25519`,
a component signature cannot be lifted out and replayed as a standalone
non-hybrid signature: the signed message itself declares it is one half of a
composite pair. The verifier checks `content_hash == SHA-256(payload)`
separately, so the signed message `M` stays small regardless of book size.

### 8.4 Roles & trust

1. **Author signature** — made with the author's identity key. Because the
   author's public key is embedded in `meta.authors[].keyset` and its
   fingerprint equals the `author_fpr` baked into the recipe ID, author
   authenticity is verifiable **fully offline**. The document owner (the
   `author_fpr` of the recipe, or of the book's `meta.identity`) MUST be among
   the signers; co-authors MAY add signatures.
2. **Server endorsement** — a composite signature by the platform key over `M`
   with `external_aad` bound to the author signature. Attests "published on this
   platform at `signed_at`."

### 8.5 Platform key management

The platform publishes its signing **key set** (current plus recently retired
keys, each with a `valid_from` and optional `valid_until`) at
`GET /api/v1/server-info`. A verifier resolves `sig-protected.kid` against this
set by fingerprint and MUST reject an endorsement whose `signed_at` falls
outside its key's window (`E.KEY.WINDOW`). Clients pin the key set per server
on first contact.

### 8.6 Identity keys and key files

An identity is a **random** 32-byte seed from a CSPRNG — never derived from a
password, because the public keys appear in every bundle and a
password-derived key could be brute-forced offline. Both component keys are
derived from the seed with domain-separated sub-seeds
(`SHA-256("crisp/key/ml-dsa-65/1" || seed)` is the ML-DSA-65 key-generation
seed `ξ`; `SHA-256("crisp/key/ed25519/1" || seed)` is the Ed25519 secret key).

For storage and backup the seed is wrapped in a **key file** (`.crispkey`,
`key-file` in the CDDL): Argon2id (m = 64 MiB, t = 3, p = 1, random 16-byte
salt) derives a 32-byte key from the passphrase, and XChaCha20-Poly1305 seals
the seed with the canonical CBOR of the header (including the `usr…` id) as
associated data. Readers MUST refuse KDF parameters outside
m ∈ [19 MiB, 1 GiB], t ∈ [1, 16], p = 1, so a hostile file cannot exhaust
memory.

### 8.7 Server login

To sign in to a server, a client answers a random challenge nonce by signing
`canonical_cbor(["crisp/login/1", audience, nonce])`, where `audience` is the
`usr…` id of the server's current platform key. Binding the audience stops a
malicious server from relaying another server's challenge.

---

## 9. Temporal metadata & cryptographic binding

Crisp carries three kinds of timestamp, each bound by a different mechanism.

| Timestamp | Where | Mutability | Bound by |
|---|---|---|---|
| `created_at` | `identity` (and `meta`) | Frozen at birth | **The ID itself** — `created_at` is in the ID preimage (§7.2), so altering it changes the recipe ID and fails the ID self-check — **and** every signature, via the payload content hash. |
| `modified_at` | `recipe` and `meta` | Updated each edit | The **revision hash** (it is inside the hashed recipe) **and** every signature (via the content hash). Each edit re-signs, so the latest `modified_at` is attested by the latest signatures. |
| `signed_at` | each `sig-protected` | Per signature | The signature over `M` (which includes `sig_protected_bstr`). |

**Trusted time.** The author's `created_at`, `modified_at`, and author
`signed_at` are claims made on the author's own clock — they prove the author
*asserted* a time, not that the time is true. The **server endorsement's
`signed_at`** is the authoritative anchor: the platform attests it saw this exact
bundle at time T. For provenance and tracking, treat the endorsement time as
trusted and the author times as claims bounded by it. The verifier enforces
`created_at ≤ modified_at` and, when an endorsement is present,
`modified_at ≤ endorsement.signed_at + skew` (§11, check D3/E3).

---

## 10. Security model

### 10.1 Non-executable & memory-safe by construction

No scripts, macros, formula/expression language, stylesheets, fonts, SVG, HTML,
or markup strings exist anywhere in the format. Rich text is pre-parsed
structure (§4). A conformant viewer never compiles, evaluates, or executes any
part of a bundle. Hard limits (§12), a non-recursive data model, and rejection
of indefinite-length CBOR make a conformant parser single-pass, total, and
bounded in time and memory.

### 10.2 Threats & mitigations

| Threat | Mitigation |
|---|---|
| Forge a recipe as another user | Recipe ID embeds `author_fpr`; the author signature proves control of that key; both are independently checkable offline. |
| Strip the post-quantum component (downgrade) | Composite ML-DSA verification requires both components; the signed `sig_protected` declares the composite `alg`, so a lone component cannot be replayed. |
| Quantum forgery | The composite signature holds as long as ML-DSA-65 *or* Ed25519 holds. |
| Tamper after signing | Signatures bind `content_hash = SHA-256(payload)`; any change breaks verification. |
| Parser-differential signature bypass | Mandatory deterministic CBOR; strict decoders reject non-canonical encodings; the signed message references exact bytes. |
| Code injection via a viewer | No executable content of any kind; `https`-only links; SVG forbidden. |
| Memory exhaustion / DoS | Hard limits; non-recursive model; indefinite-length items rejected. |
| Decompression bomb | No generic compression layer; blobs carry a declared `size`; images are size- and dimension-capped. |
| Malicious image decode | Allowlisted raster formats only; sandboxed decoding and pixel caps recommended. |
| ID collision or spoofing | 256-bit domain-separated SHA-256; ID recomputed and checked on load; 128-bit birth nonce. |
| Backdated provenance | `created_at` bound by the ID; the server endorsement provides the trusted time anchor (§9). |
| Copy-paste corruption of IDs | bech32m checksum detects typos and truncation. |

**Residual risk.** Image decoding is the largest remaining attack surface in a
viewer; implementations SHOULD use memory-safe, sandboxed decoders with pixel
caps. Signatures attest origin and integrity, not rendering safety — clients
still validate every field as untrusted input.

---

## 11. External verification & conformance

This section answers the question *"how does an outside party confirm a bundle
is fully compliant?"* — with a deterministic procedure, stable error codes, a
machine-readable verdict, and a public test suite.

### 11.1 Verification procedure (normative)

A verifier runs the checks in order. **Phase A and B failures are fatal** —
verification stops and the bundle is non-compliant. **Phase C, D, and E
failures are collected** — the verifier continues and reports all of them.

**Phase A — Framing & encoding**

| ID | Check | Error code |
|---|---|---|
| A1 | Self-describe + application tag present and correct | `E.MAGIC` |
| A2 | `container-version` is supported | `E.CONTAINER` |
| A3 | The whole file decodes as CBOR | `E.CBOR.MALFORMED` |
| A4 | Encoding is deterministic: definite-length, shortest-form, sorted keys, no duplicate keys, no floats, only `false`/`true`/`null` simple values, and the two framing tags only at the outermost frame | `E.CBOR.NONDETERMINISTIC`, `E.CBOR.INDEFINITE`, `E.CBOR.DUPKEY`, `E.CBOR.TAG` |
| A5 | Each embedded `bstr` (`payload`, every `protected`) re-decodes as deterministic CBOR with no tags | `E.CBOR.*` |

**Phase B — Schema & limits**

| ID | Check | Error code |
|---|---|---|
| B1 | Envelope and payload validate against `crisp-v1.cddl`, including no `null` anywhere, NFC text, fixed byte lengths, minimum counts, and the syntax of language tags, unit codes, slugs, and extension keys | `E.SCHEMA` |
| B2 | `v` has the reader's major version, `min_v` ≤ `v`, and `min_v` ≤ reader version | `E.VERSION` |
| B3 | Every numeric limit of §12 and every `*N` bound of the CDDL is respected | `E.LIMIT` |
| B4 | `default_language ∈ languages`, no language listed twice; `kind`/`book`/`meta.identity` presence is consistent; a recipe bundle holds exactly one recipe; the protected header's version and `doc_id` equal the payload's | `E.CONSISTENCY` |

**Phase C — Integrity & identity**

| ID | Check | Error code |
|---|---|---|
| C1 | Each blob: `blob-id == SHA-256(bytes)`; `size` equals actual length; `media_type` is allowlisted | `E.BLOB.HASH`, `E.BLOB.SIZE`, `E.BLOB.MEDIATYPE` |
| C2 | Each id has a valid bech32m checksum and the HRP its field requires | `E.ID.CHECKSUM` |
| C3 | Each recipe: `id == derive-recipe-id(identity)`, `revision == derive-revision(recipe)`, `supersedes ≠ revision`; `doc_id` is `recipes[0].id` (recipe) or `derive-book-id(meta.identity)` (book) | `E.ID.MISMATCH`, `E.REV.MISMATCH` |
| C4 | `protected-header` content-hash `== SHA-256(payload)` | `E.CONTENT.HASH` |
| C5 | Each `author.uid` derives from its keyset; each `identity.author_fpr` is the fingerprint of a listed author | `E.AUTHOR.BINDING` |
| C6 | `ref` handles are unique; every internal reference resolves (step ingredients and equipment, group indices, blob references, `recipe-entry.recipe`, `run-ref.target`) | `E.REF` |

**Phase D — Policy**

| ID | Check | Error code |
|---|---|---|
| D1 | Every `run.link` and `source.url` uses the `https` scheme and is a well-formed URL with a plain host | `E.URL.SCHEME` |
| D2 | Every localized map contains `default_language`; every key is in `languages`; `identity.language ∈ languages` | `E.L10N.DEFAULT`, `E.L10N.UNDECLARED` |
| D3 | For each recipe and for `meta`: `created_at ≤ modified_at` | `E.TIME.ORDER` |
| D4 | Each `quantity` has at least one of `min` / `text`; ranges have `min ≤ max`; durations use a UCUM time unit; temperature ranges are ordered | `E.QUANTITY` |

**Phase E — Signatures** *(cryptographic; E2 also needs the platform key set)*

| ID | Check | Error code |
|---|---|---|
| E1 | ≥1 `role: "author"` signature, including the document owner's; each is by a listed author's key and verifies over its `M` | `E.SIG.MISSING`, `E.SIG.AUTHOR` |
| E2 | Exactly one `role: "server-endorsement"`, last; its `kid` resolves to a trusted platform key; its `signed_at` is in that key's window; `binds` equals the author-signature binding; the signature verifies | `E.SIG.MISSING`, `E.KEY.UNKNOWN`, `E.KEY.WINDOW`, `E.SIG.BINDING`, `E.SIG.ENDORSEMENT` |
| E3 | Run only when E2 passed: `modified_at ≤ endorsement.signed_at + 300 s` | `E.TIME.ORDER` |

Every check appears in the report exactly once: after a fatal A/B failure the
remaining checks are reported `skipped`, as are E2 and E3 when no platform key
set is supplied. A file larger than the §12 maximum is rejected before
decoding (A3, `E.LIMIT`).

### 11.2 Compliance classes

A verifier reports compliance against one of two classes:

- **Structural** — Phases A–D pass. Phase E is run for trust reporting but is not
  required. Use this to validate drafts or to render offline.
- **Published** — Phases A–E all pass, including ≥1 valid author signature and
  exactly one valid server endorsement. **A bundle distributed by the platform
  MUST be Published-compliant** (D6).

### 11.3 Independence & offline verification

Phases A–D and check E1 require **no network**: the schema, all hashes, the
identity inputs, and the author public keys are inside the bundle. Only E2
needs the platform key set, which is distributed out of band and pinned. A
purely offline reader can therefore confirm structural validity, integrity,
identity, and authorship with zero server contact.

### 11.4 Compliance report

A verifier emits a machine-readable `compliance-report` (defined in the CDDL):
`{ compliant, class, format_version, trust, verified_at, checks[] }`, where each
entry in `checks` carries a check `id`, a `status`, and on failure the `code`
and a human-readable `detail`. `trust` is `unsigned`, `author-signed`, or
`endorsed`. This is the artifact an external party inspects to confirm
compliance.

### 11.5 Compliance suite (how to trust a verifier)

The Crisp project publishes a **compliance suite** in `spec/compliance/`: a
directory of canonical test bundles with the expected verdict for each,
generated deterministically by `crisp-core`'s test suite.

- `valid/*.crisp` — bundles that MUST verify as Published-compliant.
- `invalid/<CODE>/*.crisp` — bundles each crafted to fail exactly one check;
  a conformant verifier MUST report non-compliance with that precise error
  `code`.
- `vectors.json` — the platform key set and verification time to use, and
  for each test bundle its `SHA-256`, the failing check, and its code.

A verifier implementation is conformant only if it reproduces the expected
report for **every** vector. This is how the ecosystem trusts a verifier — and a
trusted verifier is how anyone trusts a bundle.

### 11.6 Requirement traceability

Every top-level requirement maps to one or more checks that enforce it:

| Requirement | Enforced by |
|---|---|
| Self-contained, no external fetch | B1 (schema forbids external refs), C6 |
| Deterministic / canonical encoding | A4, A5 |
| Non-executable (no SVG/markup/links abuse) | B1, C1, D1 |
| Bounded / memory-safe | A4, B3 |
| Integrity (untampered) | C1, C4 |
| Stable, collision-resistant identity | C2, C3 |
| Authorship provable offline | C5, E1 |
| Hybrid PQ-resistant signature | E1, E2 |
| Platform endorsement & trusted time | E2, E3, D3 |
| Localization complete | B4, D2 |
| Cryptographically bound timestamps | C3 (revision), C4 (content hash), E1/E2 |

### 11.7 Verifier conformance levels

- **Structural verifier** — implements Phases A–D (no cryptography); MUST pass
  every non-signature vector of the compliance suite.
- **Full verifier** — implements Phases A–E plus platform key resolution; MUST
  pass every vector.

---

## 12. Limits (single profile)

A conformant parser MUST reject a bundle that exceeds any of these.

| Limit | Value |
|---|---|
| Max bundle size | 64 MiB |
| Max recipes per bundle | 4096 |
| Max blobs | 8192 |
| Max single blob | 16 MiB |
| Max total blob bytes | 60 MiB |
| Max steps per recipe | 512 |
| Max ingredients per recipe | 512 |
| Max equipment per recipe | 256 |
| Max ingredient groups / step groups | 32 / 32 |
| Max timers per step | 8 |
| Max generic array length | 4096 |
| Max single text string | 64 KiB |
| Max title (`identity.title`, any localized title value) | 512 bytes |
| Max runs per paragraph | 1024 |
| Max paragraphs per prose block | 4096 |
| Max languages | 32 |
| Max authors | 64 |
| Max book parts / chapters per part / entries per chapter | 256 / 256 / 4096 |
| Max CBOR nesting depth | 32 |
| Integer range | signed 64-bit |

---

## 13. Controlled vocabularies

All "smart" behavior reduces to codes resolved against fixed registries — never
embedded logic.

| Domain | Vocabulary |
|---|---|
| Units | **UCUM** codes. Unit conversion is deterministic and client-side. |
| Languages | **BCP 47** tags. |
| Licenses | **SPDX** identifiers (or `NONE`). |
| Allergens, diet, course, nutrients | Enumerated in the CDDL (`allergen-code`, `diet-code`, `course-code`, `nutrient-code`). |

The `course`/`diet`/`allergen`/`nutrient` vocabularies are **open**: any
well-formed slug is valid, and a reader MUST accept a code it does not know
and render it verbatim. The listed codes are the registry. Units outside a
reader's catalogue likewise render as their code; UCUM annotations such as
`{clove}` render as their inner word.

---

## 14. Versioning & extensibility

- The format version (`v`, also in the protected header) is `major.minor.patch`
  encoded as `0xMMmmpp`. A major bump is breaking.
- The core schema is **closed**: unknown core fields are rejected (`E.SCHEMA`).
- A minor version MAY add optional fields. A writer that uses any field
  newer than 1.0 MUST set `min_v` to the version that introduced it, so an
  older reader fails cleanly with `E.VERSION` rather than `E.SCHEMA`. A writer
  that uses only 1.0 fields MUST set `min_v` to 1.0, so its bundles remain
  readable by every 1.x reader.
- `null` never appears, so decoding a bundle into a model and re-encoding it
  is lossless and revision hashes are stable across implementations.
- The ignorable surface is the `ext` map — on the payload, meta, recipes,
  ingredients, substitutes, steps, timers, equipment, parts, chapters, and
  pages — whose keys MUST be reverse-DNS namespaced (e.g.
  `com.example.feature`). Verifiers ignore unknown `ext` keys.
- A future major version may add the `MLDSA87-Ed448` composite parameter set or
  additional media types; v1 does not.

---

## 15. Registration summary

| Item | Value |
|---|---|
| File extension | `.crisp` |
| Media type | `application/crisp+cbor` |
| Debug projection media type | `application/crisp+json` (non-normative) |
| CBOR application tag | `1129468752` (placeholder; register with IANA) |
| Magic byte prefix | `D9 D9 F7` (self-describe) then the application tag |

---

## Appendix A — Pseudocode

```python
def derive_recipe_id(identity):                 # "book" => domain crisp/book-id/1, hrp "rbk"
    preimage = cbor_canonical([
        "crisp/recipe-id/1",
        identity["author_fpr"],                  # bstr(32)
        normalize(identity["title"]),            # NFC, trim, collapse whitespace
        identity["language"],
        identity["created_at"],
        identity["birth_nonce"],                 # bstr(16)
    ])
    return bech32m("rcp", b"\x01" + sha256(preimage))

def derive_revision(recipe):
    r = {k: v for k, v in recipe.items() if k != "revision"}
    return bech32m("rev", b"\x01" + sha256(cbor_canonical(r)))

def key_fingerprint(keyset):                    # = author_fpr = kid
    return sha256(cbor_canonical(["crisp/user-id/1", keyset]))

def derive_user_id(keyset):
    return bech32m("usr", b"\x01" + key_fingerprint(keyset))

def signing_message(protected_hdr, sig_protected, external_aad=b""):
    return cbor_canonical([
        "crisp/signature/1", protected_hdr, sig_protected, external_aad,
    ])

def verify(file, platform_keys=None) -> compliance_report:
    # Phases A,B  -> fatal: stop on first failure.
    # Phases C,D,E -> collect every failure, continue.
    # Published-compliant  := A..E all pass (>=1 author sig + 1 endorsement).
    # Structural-compliant := A..D all pass.
    ...
```

## Appendix B — Reject list

A conformant verifier reports non-compliance for any of: wrong magic or tag ·
unsupported container or format version · malformed or non-deterministic CBOR ·
indefinite-length items · duplicate map keys · disallowed CBOR tags · CDDL
schema violation · any §12 limit exceeded · `blob-id ≠ SHA-256(bytes)` · blob
`size` mismatch · non-allowlisted `media_type` · invalid bech32m checksum ·
`id ≠ derive-recipe-id(identity)` · `revision` mismatch · `content_hash`
mismatch · `author_fpr` not matching any author keyset · an unresolved internal
reference · a non-`https` link · a localized map missing `default_language` or
using an undeclared language · `created_at > modified_at` · a `quantity` with
neither `min` nor `text` · a failed author or endorsement signature · an
unknown or out-of-window platform key · an endorsement not bound to its author
signature.

---

*End of Crisp Recipe Bundle Specification v1.0.0 (Final). Structure is defined
normatively by `crisp-v1.cddl`.*
