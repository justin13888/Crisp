//! Bundle publication, server endorsement, retrieval, and unpublishing
//! (plan Stage 13/14; [`unpublish`]).
//!
//! Publishing is where the server earns its trust role. A client uploads an
//! author-signed `.crisp` file; the server runs the full `crisp-core` A–E
//! verification pipeline, confirms the upload belongs to the authenticated
//! account, adds its own Composite ML-DSA **endorsement**, indexes the
//! recipe metadata, and stores the endorsed bytes. Retrieval simply streams
//! stored endorsed bundles back.
//!
//! Bundles travel as `application/octet-stream`; the bytes are a `.crisp`
//! file (`application/crisp+cbor` in spec terms).

use crisp_core::codec::{decode_file, encode_file, from_canonical_cbor};
use crisp_core::identity::key_fingerprint;
use crisp_core::model::envelope::{CrispEnvelope, ProtectedHeader, SigProtected, SigRole};
use crisp_core::model::payload::{BundlePayload, Kind};
use crisp_core::model::primitives::Timestamp;
use crisp_core::verify::{ComplianceReport, PlatformKey as TrustedKey, Trust, verify};
use kynos::extract::body::binary::Binary;
use kynos::http::media::OctetStream;
use kynos::prelude::*;
use sea_orm::ActiveValue::Set;

use crate::auth::{AuthAccount, SessionToken, account_keyset};
use crate::db::bundle;
use crate::error::{
    BadRequest, Conflict, Internal, NotFound, QuotaExceeded, Removed, RemovedConflict, StorageFull,
    VerificationFailed,
};
use crate::index;
use crate::limits::{Operation, Throttle};
use crate::operation_error;
use crate::quota::Breach;
use crate::state::{CpuPool, Platform, Settings, Store};
use crate::storage::{BundleInsertError, InsertError, covering};
use crate::util::now_epoch;

use crate::routes::API_PREFIX;

pub mod unpublish;

operation_error! {
    /// What `POST /bundles` can fail with.
    pub enum PublishError {
        BadRequest, Conflict, RemovedConflict, VerificationFailed, TooManyRequests,
        QuotaExceeded, StorageFull,
    }
}

impl From<Breach> for PublishError {
    fn from(breach: Breach) -> Self {
        match breach {
            Breach::Account {
                quota,
                limit,
                used,
                requested,
            } => QuotaExceeded::new(quota, limit, used, requested).into(),
            Breach::Server {
                limit,
                used,
                requested,
            } => StorageFull::new(limit, used, requested).into(),
        }
    }
}

operation_error! {
    /// What the bundle-retrieval operations can fail with.
    pub enum FetchError { NotFound, Removed }
}

/// Answers a read that found no bundle: 410 `removed` when a tombstone of
/// `doc_id` covers it (for the latest, any tombstone of the document; for a
/// revision, one of the document or that revision), 404 otherwise.
async fn missing(
    storage: &Store,
    doc_id: &str,
    revision: Option<&str>,
    absent: &'static str,
) -> FetchError {
    let tombstones = match storage.tombstones(doc_id).await {
        Ok(tombstones) => tombstones,
        Err(error) => return error.into(),
    };
    let governing = match revision {
        Some(_) => covering(&tombstones, revision),
        None => tombstones.last(),
    };
    match governing {
        Some(tombstone) => {
            tracing::debug!(doc_id, revision, reason = %tombstone.reason, "read of a taken-down document");
            Removed::new(&tombstone.reason).into()
        }
        None => NotFound::new(absent).into(),
    }
}

/// What `GET /bundles/{doc_id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct DocPath {
    /// The bundle's document id (a `rcp…` recipe id or a book id).
    pub doc_id: String,
}

/// What `GET /bundles/{doc_id}/{revision}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct RevisionPath {
    /// The bundle's document id.
    pub doc_id: String,
    /// The stored revision: a recipe's `rev…` id, or a book's content hash.
    pub revision: String,
}

/// The stored `(kind, revision)` pair identifying one published version.
struct StoredRevision {
    /// `"recipe"` or `"book"`.
    kind: &'static str,
    /// The recipe revision id, or the content hash for a book.
    revision: String,
}

/// Derives the stored revision key for a payload. A recipe bundle is keyed by
/// its recipe's `rev…` id; a book — which has no single revision — is keyed
/// by its content hash.
fn stored_revision(
    payload: &BundlePayload,
    content_hash: &str,
) -> Result<StoredRevision, BadRequest> {
    match payload.kind {
        Kind::Recipe => {
            let recipe = payload
                .recipes
                .first()
                .ok_or_else(|| BadRequest::new("recipe bundle has no recipe"))?;
            Ok(StoredRevision {
                kind: "recipe",
                revision: recipe.revision.0.clone(),
            })
        }
        Kind::Book => Ok(StoredRevision {
            kind: "book",
            revision: content_hash.to_owned(),
        }),
    }
}

/// Runs the A–E verification pipeline over an upload.
///
/// The platform's own keys are passed so E2 runs: an upload is expected to be
/// unendorsed (E2 then reports the missing endorsement, which does not affect
/// structural compliance), and an upload already carrying *our* endorsement
/// is recognized as such. This is the single seam where publication meets
/// `crisp_core::verify::verify`.
#[must_use]
pub fn verify_upload(bytes: &[u8], platform_keys: &[TrustedKey], now: i64) -> ComplianceReport {
    verify(
        bytes,
        platform_keys,
        Timestamp(u64::try_from(now).unwrap_or(0)),
    )
}

/// Refuses an upload that already carries any `server-endorsement`
/// signature — ours or anyone's. Endorsing it again would produce a bundle
/// with two endorsements, which spec D6 forbids.
fn reject_pre_endorsed(envelope: &CrispEnvelope) -> Result<(), BadRequest> {
    for signature in &envelope.signed_bundle.signatures {
        let protected = from_canonical_cbor::<SigProtected>(&signature.protected)
            .map_err(|e| BadRequest::new(format!("unreadable signature header: {e}")))?;
        if protected.role == SigRole::ServerEndorsement {
            return Err(BadRequest::new(
                "bundle already carries a server endorsement; publish the author-signed bundle",
            ));
        }
    }
    Ok(())
}

/// Confirms an author signature in `envelope` was made by the caller's key —
/// i.e. the uploader is genuinely an author of the bundle they are
/// publishing.
fn caller_is_author(envelope: &CrispEnvelope, caller: &AuthAccount) -> Result<(), PublishError> {
    let fingerprint = key_fingerprint(&account_keyset(&caller.account))
        .map_err(|e| Internal::logged("fingerprint of a stored account key", &e))?;
    for signature in &envelope.signed_bundle.signatures {
        let Ok(sig_protected) = from_canonical_cbor::<SigProtected>(&signature.protected) else {
            continue;
        };
        if sig_protected.role == SigRole::Author && sig_protected.kid == fingerprint {
            return Ok(());
        }
    }
    Err(BadRequest::new("the authenticated account is not an author of this bundle").into())
}

/// A verified upload, endorsed and ready to store.
struct Endorsed {
    /// The decoded bundle payload, for the search index.
    payload: BundlePayload,
    /// The hex content hash from the protected header.
    content_hash: String,
    /// The `(kind, revision)` the bundle is stored under.
    stored: StoredRevision,
    /// The endorsed `.crisp` file.
    endorsed_bytes: Vec<u8>,
}

/// The CPU-bound half of publication: runs A–E verification, checks the
/// upload is unendorsed, author-signed, and authored by `caller`, then adds
/// the platform endorsement and re-encodes the file.
///
/// The upload is decoded again after [`verify_upload`], which reports on the
/// bytes but does not hand back what it decoded; decoding is linear in the
/// upload and small beside the signature checks.
fn verify_and_endorse(
    body: &[u8],
    platform: &Platform,
    caller: &AuthAccount,
    now: i64,
) -> Result<Endorsed, PublishError> {
    let report = verify_upload(body, &platform.platform_keys(), now);
    if !report.compliant {
        tracing::info!(
            failures = report.failures().len(),
            "publish rejected: not compliant"
        );
        return Err(VerificationFailed::new(report).into());
    }

    let envelope = decode_file(body).map_err(BadRequest::from)?;
    reject_pre_endorsed(&envelope).inspect_err(|_| {
        tracing::info!("publish rejected: upload is already endorsed");
    })?;
    if report.trust != Trust::AuthorSigned {
        tracing::info!(trust = ?report.trust, "publish rejected: no valid author signature");
        return Err(BadRequest::new("bundle carries no valid author signature").into());
    }

    let payload: BundlePayload =
        from_canonical_cbor(&envelope.signed_bundle.payload).map_err(BadRequest::from)?;
    let protected: ProtectedHeader =
        from_canonical_cbor(&envelope.signed_bundle.protected).map_err(BadRequest::from)?;

    caller_is_author(&envelope, caller).inspect_err(|_| {
        tracing::info!("publish rejected: caller is not an author");
    })?;

    let content_hash = hex::encode(&protected.content_hash);
    let stored = stored_revision(&payload, &content_hash)?;

    // Add the platform endorsement and re-serialize.
    let endorsed = platform
        .endorse(&envelope, now)
        .map_err(|e| Internal::logged("endorsing a verified bundle", &e))?;
    let endorsed_bytes =
        encode_file(&endorsed).map_err(|e| Internal::logged("encoding an endorsed bundle", &e))?;
    Ok(Endorsed {
        payload,
        content_hash,
        stored,
        endorsed_bytes,
    })
}

/// Verifies, endorses, indexes, and stores an author-signed bundle.
///
/// Answers 201 with the endorsed `.crisp` bytes; `Location` names the exact
/// revision just published. A bundle that fails verification is refused with
/// its compliance report. Throttled per account (see [`crate::limits`]).
///
/// The endorsed bundle counts against the publisher's storage quotas: one
/// that would exceed them is refused 403 `quota_exceeded`, naming the quota
/// and the numbers, and one that would exceed the server's total cap is
/// refused 507 `storage_full` (see [`crate::quota`]).
///
/// A document an administrator took down is refused 409 `removed` until the
/// takedown is lifted; so is a revision taken down on its own, while other
/// revisions of its document may still be published.
#[kynos::post("/bundles")]
#[tracing::instrument(level = "info", skip_all, fields(usr_id))]
pub async fn publish(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
    Inject(settings): Inject<Settings>,
    Inject(platform): Inject<Platform>,
    Inject(throttle): Inject<Throttle>,
    Inject(cpu): Inject<CpuPool>,
    body: Binary<OctetStream>,
) -> Result<Created<Binary<OctetStream>>, PublishError> {
    let caller = auth.into_inner().resolve()?;
    let usr_id = caller.account.usr_id.clone();
    tracing::Span::current().record("usr_id", usr_id.as_str());
    throttle.check(Operation::Publish, &usr_id)?;
    let body = body.into_inner();
    if body.is_empty() {
        return Err(BadRequest::new("empty request body").into());
    }
    let now = now_epoch();
    tracing::debug!(bytes = body.len(), "verifying upload");

    // Verification and endorsement are CPU-bound; they run on the bounded
    // pool so a burst of large uploads cannot stall the async workers.
    let Endorsed {
        payload,
        content_hash,
        stored,
        endorsed_bytes,
    } = cpu
        .run("publish", move || {
            verify_and_endorse(&body, &platform, &caller, now)
        })
        .await??;
    let doc_id = payload.doc_id.0.clone();
    let tombstones = storage.tombstones(&doc_id).await?;
    if let Some(tombstone) = covering(&tombstones, Some(&stored.revision)) {
        tracing::info!(%doc_id, revision = %stored.revision, reason = %tombstone.reason, "publish rejected: taken down");
        return Err(RemovedConflict::new(&tombstone.reason).into());
    }
    let size_bytes = i64::try_from(endorsed_bytes.len())
        .map_err(|e| Internal::logged("sizing an endorsed bundle", &e))?;

    let row = bundle::ActiveModel {
        content_hash: Set(content_hash.clone()),
        doc_id: Set(doc_id.clone()),
        revision: Set(stored.revision.clone()),
        kind: Set(stored.kind.to_owned()),
        format_version: Set(i64::try_from(payload.v).unwrap_or(i64::MAX)),
        raw: Set(endorsed_bytes.clone()),
        size_bytes: Set(size_bytes),
        published_by: Set(usr_id.clone()),
        published_at: Set(now),
        endorsed_at: Set(now),
    };
    let index_rows = index::extract(&payload)
        .into_iter()
        .map(|r| r.into_active_model(&content_hash))
        .collect::<Vec<_>>();
    let indexed = index_rows.len();
    // Built before the insert, so a refusal here never follows a stored bundle.
    let location = Location::try_from(format!("{API_PREFIX}/bundles/{doc_id}/{}", stored.revision))
        .map_err(|e| Internal::logged("building a published bundle's Location", &e))?;
    match storage
        .insert_bundle(row, index_rows, &settings.quota)
        .await
    {
        Ok(()) => {}
        Err(BundleInsertError::Quota(breach)) => {
            tracing::info!(%doc_id, revision = %stored.revision, size_bytes, %breach, "publish rejected: storage quota");
            return Err(breach.into());
        }
        // The unique constraints on `content_hash` and `(doc_id, revision)`
        // are the arbiter, so concurrent duplicate publishes yield one 201
        // and one 409, never a 500.
        Err(BundleInsertError::Insert(InsertError::Duplicate(detail))) => {
            tracing::info!(%doc_id, revision = %stored.revision, %detail, "publish rejected: duplicate revision");
            return Err(Conflict::new("this document revision is already published").into());
        }
        Err(BundleInsertError::Insert(InsertError::Db(error))) => return Err(error.into()),
    }

    tracing::info!(%doc_id, revision = %stored.revision, kind = stored.kind, indexed, "bundle published");
    Ok(Created::at(location, Binary::new(endorsed_bytes)))
}

/// Returns the latest endorsed bundle for a document.
///
/// Answers 410 `removed`, with the reason category, once an administrator
/// has taken every remaining revision down.
#[kynos::get("/bundles/{doc_id}")]
#[tracing::instrument(level = "debug", skip(storage))]
pub async fn get_latest(
    Inject(storage): Inject<Store>,
    Path(path): Path<DocPath>,
) -> Result<Binary<OctetStream>, FetchError> {
    match storage.bundle_latest(&path.doc_id).await? {
        Some(bundle) => Ok(Binary::new(bundle.raw)),
        None => Err(missing(
            &storage,
            &path.doc_id,
            None,
            "no bundle with that document id",
        )
        .await),
    }
}

/// Returns one exact endorsed revision of a document.
///
/// Answers 410 `removed`, with the reason category, when an administrator
/// took the revision or its whole document down.
#[kynos::get("/bundles/{doc_id}/{revision}")]
#[tracing::instrument(level = "debug", skip(storage))]
pub async fn get_exact(
    Inject(storage): Inject<Store>,
    Path(path): Path<RevisionPath>,
) -> Result<Binary<OctetStream>, FetchError> {
    match storage.bundle_exact(&path.doc_id, &path.revision).await? {
        Some(bundle) => Ok(Binary::new(bundle.raw)),
        None => {
            let absent = "no bundle with that document id and revision";
            Err(missing(&storage, &path.doc_id, Some(&path.revision), absent).await)
        }
    }
}
