//! Bundle publication, server endorsement, and retrieval (plan Stage 13/14).
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
use crisp_core::model::meta::PublicKeySet;
use crisp_core::model::payload::{BundlePayload, Kind};
use crisp_core::model::primitives::Timestamp;
use crisp_core::verify::{ComplianceReport, Trust, verify};
use kynos::extract::body::binary::Binary;
use kynos::http::media::OctetStream;
use kynos::prelude::*;
use sea_orm::ActiveValue::Set;

use crate::auth::{AuthAccount, SessionToken, account_keyset};
use crate::db::bundle;
use crate::error::{BadRequest, Conflict, Internal, NotFound, VerificationFailed};
use crate::index;
use crate::operation_error;
use crate::state::{Platform, Store};
use crate::storage::InsertError;
use crate::util::now_epoch;

use crate::routes::API_PREFIX;

operation_error! {
    /// What `POST /bundles` can fail with.
    pub enum PublishError { BadRequest, Conflict, VerificationFailed }
}

operation_error! {
    /// What the bundle-retrieval operations can fail with.
    pub enum FetchError { NotFound }
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
pub fn verify_upload(bytes: &[u8], platform_keys: &[PublicKeySet], now: i64) -> ComplianceReport {
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

/// Verifies, endorses, indexes, and stores an author-signed bundle.
///
/// Answers 201 with the endorsed `.crisp` bytes; `Location` names the exact
/// revision just published. A bundle that fails verification is refused with
/// its compliance report.
#[kynos::post("/bundles")]
#[tracing::instrument(level = "info", skip_all, fields(usr_id))]
pub async fn publish(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
    Inject(platform): Inject<Platform>,
    body: Binary<OctetStream>,
) -> Result<Created<Binary<OctetStream>>, PublishError> {
    let caller = auth.into_inner();
    let usr_id = caller.account.usr_id.clone();
    tracing::Span::current().record("usr_id", usr_id.as_str());
    let body = body.into_inner();
    if body.is_empty() {
        return Err(BadRequest::new("empty request body").into());
    }
    let now = now_epoch();
    tracing::debug!(bytes = body.len(), "verifying upload");

    let report = verify_upload(&body, &platform.platform_keys(), now);
    if !report.compliant {
        tracing::info!(
            failures = report.failures().len(),
            "publish rejected: not compliant"
        );
        return Err(VerificationFailed::new(report).into());
    }

    let envelope = decode_file(&body).map_err(BadRequest::from)?;
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

    caller_is_author(&envelope, &caller).inspect_err(|_| {
        tracing::info!("publish rejected: caller is not an author");
    })?;

    let content_hash = hex::encode(&protected.content_hash);
    let doc_id = payload.doc_id.0.clone();
    let stored = stored_revision(&payload, &content_hash)?;

    // Add the platform endorsement and re-serialize.
    let endorsed = platform
        .endorse(&envelope, now)
        .map_err(|e| Internal::logged("endorsing a verified bundle", &e))?;
    let endorsed_bytes =
        encode_file(&endorsed).map_err(|e| Internal::logged("encoding an endorsed bundle", &e))?;

    let row = bundle::ActiveModel {
        content_hash: Set(content_hash.clone()),
        doc_id: Set(doc_id.clone()),
        revision: Set(stored.revision.clone()),
        kind: Set(stored.kind.to_owned()),
        format_version: Set(i64::try_from(payload.v).unwrap_or(i64::MAX)),
        raw: Set(endorsed_bytes.clone()),
        published_by: Set(usr_id.clone()),
        published_at: Set(now),
        endorsed_at: Set(now),
    };
    let index_rows = index::extract(&payload)
        .into_iter()
        .map(|r| r.into_active_model(&content_hash))
        .collect::<Vec<_>>();
    let indexed = index_rows.len();
    match storage.insert_bundle(row, index_rows).await {
        Ok(()) => {}
        // The unique constraints on `content_hash` and `(doc_id, revision)`
        // are the arbiter, so concurrent duplicate publishes yield one 201
        // and one 409, never a 500.
        Err(InsertError::Duplicate(detail)) => {
            tracing::info!(%doc_id, revision = %stored.revision, %detail, "publish rejected: duplicate revision");
            return Err(Conflict::new("this document revision is already published").into());
        }
        Err(InsertError::Db(error)) => return Err(error.into()),
    }

    tracing::info!(%doc_id, revision = %stored.revision, kind = stored.kind, indexed, "bundle published");
    let location = format!("{API_PREFIX}/bundles/{doc_id}/{}", stored.revision);
    Ok(Created::at(location, Binary::new(endorsed_bytes)))
}

/// Returns the latest endorsed bundle for a document.
#[kynos::get("/bundles/{doc_id}")]
#[tracing::instrument(level = "debug", skip(storage))]
pub async fn get_latest(
    Inject(storage): Inject<Store>,
    Path(path): Path<DocPath>,
) -> Result<Binary<OctetStream>, FetchError> {
    let bundle = storage
        .bundle_latest(&path.doc_id)
        .await?
        .ok_or_else(|| NotFound::new("no bundle with that document id"))?;
    Ok(Binary::new(bundle.raw))
}

/// Returns one exact endorsed revision of a document.
#[kynos::get("/bundles/{doc_id}/{revision}")]
#[tracing::instrument(level = "debug", skip(storage))]
pub async fn get_exact(
    Inject(storage): Inject<Store>,
    Path(path): Path<RevisionPath>,
) -> Result<Binary<OctetStream>, FetchError> {
    let bundle = storage
        .bundle_exact(&path.doc_id, &path.revision)
        .await?
        .ok_or_else(|| NotFound::new("no bundle with that document id and revision"))?;
    Ok(Binary::new(bundle.raw))
}
