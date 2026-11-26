//! The storage refusals: a publish past an account quota or the server cap.

use kynos::ApiError;

use super::{PROBLEM_BASE, code};
use crate::quota::QuotaKind;

/// A publish would take the account past one of its storage quotas (see
/// [`crate::quota`]). Deleting old revisions or documents frees room.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{detail}")]
#[problem(
    base = PROBLEM_BASE,
    status = 403,
    title = "Quota exceeded",
    extension(code = code::QUOTA_EXCEEDED)
)]
pub struct QuotaExceeded {
    /// Which quota, and by how much.
    detail: String,
    /// `bytes` or `bundles`.
    #[problem(extension)]
    quota: QuotaKind,
    /// The quota's limit.
    #[problem(extension)]
    limit: u64,
    /// What the account already uses.
    #[problem(extension)]
    used: u64,
    /// What the refused publish would add.
    #[problem(extension)]
    requested: u64,
}

impl QuotaExceeded {
    /// A refusal for crossing `quota`: `used` plus `requested` exceeds
    /// `limit`.
    #[must_use]
    pub fn new(quota: QuotaKind, limit: u64, used: u64, requested: u64) -> Self {
        let what = match quota {
            QuotaKind::Bytes => "storage quota",
            QuotaKind::Bundles => "bundle quota",
        };
        Self {
            detail: format!(
                "publishing this would exceed your {what} ({used} used, {requested} requested, \
                 limit {limit}); delete old revisions or documents to make room"
            ),
            quota,
            limit,
            used,
            requested,
        }
    }
}

/// A publish would take the server past its total storage cap
/// (`CRISP_MAX_STORED_BYTES`). The numbers are logged, never exposed: they
/// describe every account's storage, not the caller's.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("the server has no room for this bundle; try again later")]
#[problem(
    base = PROBLEM_BASE,
    status = 507,
    title = "Storage full",
    extension(code = code::STORAGE_FULL)
)]
pub struct StorageFull;

impl StorageFull {
    /// Logs the cap, the stored total, and the refused size at WARN level
    /// and returns the opaque 507.
    #[must_use]
    pub fn new(limit: u64, used: u64, requested: u64) -> Self {
        tracing::warn!(limit, used, requested, "server storage cap reached");
        Self
    }
}
