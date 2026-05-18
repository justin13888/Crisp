//! Bundle publication, server endorsement, and retrieval (plan Stage 13/14).
//!
//! Publishing is where the server earns its trust role. A client uploads an
//! author-signed `.crisp` file; the server runs the full `crisp-core` A–E
//! verification pipeline, confirms the upload belongs to the authenticated
//! account, adds its own Composite ML-DSA **endorsement**, indexes the
//! recipe metadata, and stores the endorsed bytes. Retrieval simply streams
//! stored endorsed bundles back.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use crisp_core::codec::{decode_file, encode_file, from_canonical_cbor};
use crisp_core::identity::key_fingerprint;
use crisp_core::model::envelope::{CrispEnvelope, ProtectedHeader, SigProtected, SigRole};
use crisp_core::model::payload::{BundlePayload, Kind};
use crisp_core::model::primitives::Timestamp;
use crisp_core::verify::{Trust, verify};
use sea_orm::ActiveValue::Set;

use crate::auth::{AuthAccount, account_keyset};
use crate::db::bundle;
use crate::error::{ApiError, ApiResult};
use crate::index;
use crate::state::AppState;
use crate::util::now_epoch;

/// The Crisp media type (spec §0).
const MEDIA_TYPE: &str = "application/crisp+cbor";

/// Renders bundle bytes as an HTTP response with the Crisp media type.
fn bundle_response(status: StatusCode, bytes: Vec<u8>) -> impl IntoResponse {
    (status, [(header::CONTENT_TYPE, MEDIA_TYPE)], bytes)
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
fn stored_revision(payload: &BundlePayload, content_hash: &str) -> ApiResult<StoredRevision> {
    match payload.kind {
        Kind::Recipe => {
            let recipe = payload
                .recipes
                .first()
                .ok_or_else(|| ApiError::BadRequest("recipe bundle has no recipe".into()))?;
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

/// Confirms an author signature in `envelope` was made by `keyset` — i.e. the
/// uploader is genuinely an author of the bundle they are publishing.
fn caller_is_author(envelope: &CrispEnvelope, account: &AuthAccount) -> ApiResult<()> {
    let fingerprint = key_fingerprint(&account_keyset(&account.account))?;
    for signature in &envelope.signed_bundle.signatures {
        let Ok(sig_protected) = from_canonical_cbor::<SigProtected>(&signature.protected) else {
            continue;
        };
        if sig_protected.role == SigRole::Author && sig_protected.kid == fingerprint {
            return Ok(());
        }
    }
    Err(ApiError::BadRequest(
        "the authenticated account is not an author of this bundle".into(),
    ))
}

/// `POST /api/v1/bundles` — verifies, endorses, indexes, and stores a bundle.
pub async fn publish(
    State(state): State<AppState>,
    account: AuthAccount,
    body: Bytes,
) -> ApiResult<impl IntoResponse> {
    if body.is_empty() {
        return Err(ApiError::BadRequest("empty request body".into()));
    }
    let now = now_epoch();
    let now_ts = Timestamp(u64::try_from(now).unwrap_or(0));

    // Run the full A–E pipeline. Platform keys are empty here: the upload is
    // not endorsed yet, so E2 is skipped.
    let report = verify(&body, &[], now_ts);
    if !report.compliant {
        tracing::info!(usr_id = %account.account.usr_id, "publish rejected: not compliant");
        return Err(ApiError::Verification(Box::new(report)));
    }
    match report.trust {
        Trust::AuthorSigned => {}
        Trust::Unsigned => {
            return Err(ApiError::BadRequest(
                "bundle carries no valid author signature".into(),
            ));
        }
        Trust::Endorsed => {
            return Err(ApiError::Conflict(
                "bundle is already endorsed; publish an author-signed bundle".into(),
            ));
        }
    }

    let envelope = decode_file(&body)?;
    let payload: BundlePayload = from_canonical_cbor(&envelope.signed_bundle.payload)?;
    let protected: ProtectedHeader = from_canonical_cbor(&envelope.signed_bundle.protected)?;

    caller_is_author(&envelope, &account)?;

    let content_hash = hex::encode(&protected.content_hash);
    let doc_id = payload.doc_id.0.clone();
    let stored = stored_revision(&payload, &content_hash)?;

    if state
        .storage
        .revision_exists(&doc_id, &stored.revision)
        .await?
    {
        return Err(ApiError::Conflict(
            "this document revision is already published".into(),
        ));
    }

    // Add the platform endorsement and re-serialize.
    let endorsed = state.platform.endorse(&envelope, now)?;
    let endorsed_bytes = encode_file(&endorsed)?;

    let row = bundle::ActiveModel {
        content_hash: Set(content_hash.clone()),
        doc_id: Set(doc_id.clone()),
        revision: Set(stored.revision.clone()),
        kind: Set(stored.kind.to_owned()),
        format_version: Set(i64::try_from(payload.v).unwrap_or(0)),
        raw: Set(endorsed_bytes.clone()),
        published_by: Set(account.account.usr_id.clone()),
        published_at: Set(now),
        endorsed_at: Set(now),
    };
    let index_rows = index::extract(&payload)
        .into_iter()
        .map(|r| r.into_active_model(&content_hash))
        .collect();
    state.storage.insert_bundle(row, index_rows).await?;

    tracing::info!(%doc_id, revision = %stored.revision, usr_id = %account.account.usr_id, "bundle published");
    Ok(bundle_response(StatusCode::CREATED, endorsed_bytes))
}

/// `GET /api/v1/bundles/{doc_id}` — the latest endorsed bundle for a document.
pub async fn get_latest(
    State(state): State<AppState>,
    Path(doc_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let bundle = state
        .storage
        .bundle_latest(&doc_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(bundle_response(StatusCode::OK, bundle.raw))
}

/// `GET /api/v1/bundles/{doc_id}/{revision}` — one exact endorsed revision.
pub async fn get_exact(
    State(state): State<AppState>,
    Path((doc_id, revision)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    let bundle = state
        .storage
        .bundle_exact(&doc_id, &revision)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(bundle_response(StatusCode::OK, bundle.raw))
}
