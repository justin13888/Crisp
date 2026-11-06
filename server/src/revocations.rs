//! `GET /api/v1/revocations`: the endorsements this server revoked (spec
//! §8.8), signed by its current platform key.
//!
//! A takedown revokes the endorsement of every bundle it removes. The list
//! only grows: lifting a takedown lets a document be published again as a
//! new bundle, but never restores an endorsement a client has seen revoked.

use crisp_core::model::primitives::Timestamp;
use crisp_core::verify::Revocation;
use kynos::http::forwarded::Forwarded;
use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::db::revocation;
use crate::error::Internal;
use crate::limits::{Operation, Throttle, client_key};
use crate::operation_error;
use crate::state::{Platform, Store};

/// One revoked endorsement.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct RevocationInfo {
    /// The revoked bundle's `content_hash` (`SHA-256(payload)`),
    /// hex-encoded.
    pub content_hash: String,
    /// When the endorsement was revoked (epoch seconds).
    pub revoked_at: i64,
}

/// A signature over the published revocation list.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct RevocationsSignatureInfo {
    /// The `usr…` id of the signing key: the server's current key.
    pub usr_id: String,
    /// The Composite ML-DSA signature over
    /// `canonical_cbor(["crisp/platform-revocations/1", .cbor platform-revocations])`,
    /// hex-encoded.
    pub sig: String,
}

/// The body of `GET /api/v1/revocations`.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct Revocations {
    /// Every endorsement this server revoked, oldest first. The signatures
    /// cover the list in this order.
    pub revocations: Vec<RevocationInfo>,
    /// Signatures over `revocations`, by the current platform key.
    pub signatures: Vec<RevocationsSignatureInfo>,
}

operation_error! {
    /// What `GET /revocations` can fail with.
    pub enum RevocationsError { TooManyRequests }
}

/// The core form of one stored revocation.
///
/// # Errors
///
/// Returns [`Internal`] when the stored hash is not hex or the time is
/// negative.
pub fn revocation_of(row: &revocation::Model) -> Result<Revocation, Internal> {
    Ok(Revocation {
        content_hash: hex::decode(&row.content_hash)
            .map_err(|e| Internal::logged("revocations.content_hash", &e))?,
        revoked_at: Timestamp(
            u64::try_from(row.revoked_at)
                .map_err(|e| Internal::logged("revocations.revoked_at", &e))?,
        ),
    })
}

/// Lists every endorsement this server revoked, with a signature by its
/// current platform key that a client verifies against its pin before it
/// applies the list. Shares the browse and search request budget, since each
/// answer is signed afresh.
#[kynos::get("/revocations")]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn revocations(
    Inject(storage): Inject<Store>,
    Inject(platform): Inject<Platform>,
    Inject(throttle): Inject<Throttle>,
    origin: Forwarded,
) -> Result<Json<Revocations>, RevocationsError> {
    throttle.check(Operation::Query, &client_key(&origin))?;
    let rows = storage.revocations().await?;
    let list = rows
        .iter()
        .map(revocation_of)
        .collect::<Result<Vec<_>, _>>()?;
    let sig = platform
        .sign_revocations(&list)
        .map_err(|e| Internal::logged("signing the revocation list", &e))?;
    tracing::debug!(revocations = list.len(), "revocation list served");
    Ok(Json(Revocations {
        revocations: rows
            .into_iter()
            .map(|row| RevocationInfo {
                content_hash: row.content_hash,
                revoked_at: row.revoked_at,
            })
            .collect(),
        signatures: vec![RevocationsSignatureInfo {
            usr_id: platform.user_id().to_owned(),
            sig: hex::encode(sig),
        }],
    }))
}
