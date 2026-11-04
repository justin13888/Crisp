//! Storage quotas: how much one account may store, and how much the whole
//! server may.
//!
//! Every stored bundle counts its endorsed size (`bundles.size_bytes`)
//! against its publisher. A publish is refused when it would take the
//! account past [`Quota::bytes_per_account`] or
//! [`Quota::bundles_per_account`] (403 `quota_exceeded`), or the server past
//! [`Quota::max_stored_bytes`] (507 `storage_full`). The check runs inside
//! the insert transaction with the publisher's `accounts` row locked (see
//! [`crate::storage::Storage::insert_bundle`]), so concurrent publishes by
//! one account are serialized and cannot overshoot together.
//!
//! [`check`](Quota::check) is the single decision; storage only supplies the
//! numbers. `GET /account/usage` reports an account's standing.

use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::auth::SessionToken;
use crate::operation_error;
use crate::state::{Settings, Store};

/// The default per-account byte quota (256 MiB).
pub const DEFAULT_QUOTA_BYTES_PER_ACCOUNT: u64 = 256 * 1024 * 1024;
/// The default per-account bundle quota.
pub const DEFAULT_QUOTA_BUNDLES_PER_ACCOUNT: u64 = 5000;

/// The storage limits a publish is checked against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quota {
    /// The most endorsed bytes one account may store
    /// (`CRISP_QUOTA_BYTES_PER_ACCOUNT`).
    pub bytes_per_account: u64,
    /// The most bundles one account may store
    /// (`CRISP_QUOTA_BUNDLES_PER_ACCOUNT`).
    pub bundles_per_account: u64,
    /// The most endorsed bytes the whole server may store, or `None` for no
    /// cap (`CRISP_MAX_STORED_BYTES`).
    pub max_stored_bytes: Option<u64>,
}

impl Default for Quota {
    fn default() -> Self {
        Self {
            bytes_per_account: DEFAULT_QUOTA_BYTES_PER_ACCOUNT,
            bundles_per_account: DEFAULT_QUOTA_BUNDLES_PER_ACCOUNT,
            max_stored_bytes: None,
        }
    }
}

/// What one account currently stores.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// The total endorsed size of its bundles, in bytes.
    pub bytes: u64,
    /// How many bundles it has published.
    pub bundles: u64,
}

/// Which per-account limit a publish would cross.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaKind {
    /// [`Quota::bytes_per_account`].
    Bytes,
    /// [`Quota::bundles_per_account`].
    Bundles,
}

impl QuotaKind {
    /// The name the limit has in a `quota_exceeded` problem.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bytes => "bytes",
            Self::Bundles => "bundles",
        }
    }
}

/// Why a publish was refused for storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Breach {
    /// The publisher would exceed one of its own quotas.
    #[error("{} quota exceeded: {used} used + {requested} requested > {limit}", .quota.as_str())]
    Account {
        /// Which quota.
        quota: QuotaKind,
        /// The quota's limit.
        limit: u64,
        /// What the account already uses.
        used: u64,
        /// What the publish adds (its size in bytes, or one bundle).
        requested: u64,
    },
    /// The server would exceed its total storage cap.
    #[error("server storage full: {used} used + {requested} requested > {limit}")]
    Server {
        /// The cap, in bytes.
        limit: u64,
        /// The bytes stored by every account together.
        used: u64,
        /// The size of the refused bundle.
        requested: u64,
    },
}

impl Quota {
    /// Decides whether a bundle of `size` bytes may be stored by an account
    /// whose standing is `account`, on a server storing `server_bytes` in
    /// total (`None` when there is no cap to check it against).
    ///
    /// The byte quota is checked before the bundle quota, and both before
    /// the server cap, so a refusal names what the publisher can act on.
    ///
    /// # Errors
    ///
    /// Returns the first [`Breach`] the publish would cause.
    pub fn check(
        &self,
        account: Usage,
        server_bytes: Option<u64>,
        size: u64,
    ) -> Result<(), Breach> {
        let over = |used: u64, requested: u64, limit: u64| used.saturating_add(requested) > limit;
        if over(account.bytes, size, self.bytes_per_account) {
            return Err(Breach::Account {
                quota: QuotaKind::Bytes,
                limit: self.bytes_per_account,
                used: account.bytes,
                requested: size,
            });
        }
        if over(account.bundles, 1, self.bundles_per_account) {
            return Err(Breach::Account {
                quota: QuotaKind::Bundles,
                limit: self.bundles_per_account,
                used: account.bundles,
                requested: 1,
            });
        }
        if let (Some(limit), Some(used)) = (self.max_stored_bytes, server_bytes)
            && over(used, size, limit)
        {
            return Err(Breach::Server {
                limit,
                used,
                requested: size,
            });
        }
        Ok(())
    }
}

/// The answer to `GET /account/usage`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Schema)]
pub struct AccountUsage {
    /// The total endorsed size of the account's bundles, in bytes.
    pub bytes_used: u64,
    /// The most bytes the account may store.
    pub bytes_limit: u64,
    /// How many bundles the account has published.
    pub bundles_used: u64,
    /// The most bundles the account may store.
    pub bundles_limit: u64,
}

operation_error! {
    /// What `GET /account/usage` can fail with.
    pub enum UsageError {}
}

/// Reports how much the signed-in account stores against its quotas.
#[kynos::get("/account/usage")]
#[tracing::instrument(level = "debug", skip_all, fields(usr_id))]
pub async fn account_usage(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
    Inject(settings): Inject<Settings>,
) -> Result<Json<AccountUsage>, UsageError> {
    let caller = auth.into_inner().resolve()?;
    let usr_id = caller.account.usr_id;
    tracing::Span::current().record("usr_id", usr_id.as_str());
    let usage = storage.account_usage(&usr_id).await?;
    tracing::debug!(
        bytes = usage.bytes,
        bundles = usage.bundles,
        "account usage"
    );
    Ok(Json(AccountUsage {
        bytes_used: usage.bytes,
        bytes_limit: settings.quota.bytes_per_account,
        bundles_used: usage.bundles,
        bundles_limit: settings.quota.bundles_per_account,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUOTA: Quota = Quota {
        bytes_per_account: 100,
        bundles_per_account: 3,
        max_stored_bytes: Some(1000),
    };

    fn usage(bytes: u64, bundles: u64) -> Usage {
        Usage { bytes, bundles }
    }

    #[test]
    fn a_publish_that_fits_exactly_is_allowed() {
        assert_eq!(QUOTA.check(usage(60, 2), Some(960), 40), Ok(()));
    }

    #[test]
    fn crossing_the_byte_quota_names_bytes() {
        assert_eq!(
            QUOTA.check(usage(61, 0), Some(0), 40),
            Err(Breach::Account {
                quota: QuotaKind::Bytes,
                limit: 100,
                used: 61,
                requested: 40,
            })
        );
    }

    #[test]
    fn crossing_the_bundle_quota_names_bundles() {
        assert_eq!(
            QUOTA.check(usage(0, 3), Some(0), 1),
            Err(Breach::Account {
                quota: QuotaKind::Bundles,
                limit: 3,
                used: 3,
                requested: 1,
            })
        );
    }

    #[test]
    fn the_byte_quota_is_reported_before_the_bundle_quota() {
        let breach = QUOTA.check(usage(100, 3), Some(0), 1).unwrap_err();
        assert!(matches!(
            breach,
            Breach::Account {
                quota: QuotaKind::Bytes,
                ..
            }
        ));
    }

    #[test]
    fn crossing_the_server_cap_is_a_server_breach() {
        assert_eq!(
            QUOTA.check(usage(0, 0), Some(990), 11),
            Err(Breach::Server {
                limit: 1000,
                used: 990,
                requested: 11,
            })
        );
    }

    #[test]
    fn no_cap_or_no_total_skips_the_server_check() {
        let uncapped = Quota {
            max_stored_bytes: None,
            ..QUOTA
        };
        assert_eq!(uncapped.check(usage(0, 0), Some(u64::MAX), 1), Ok(()));
        assert_eq!(QUOTA.check(usage(0, 0), None, 1), Ok(()));
    }

    #[test]
    fn sums_saturate_instead_of_wrapping() {
        assert!(QUOTA.check(usage(u64::MAX, 0), Some(0), 1).is_err());
    }

    #[test]
    fn the_defaults_match_the_documented_policy() {
        let quota = Quota::default();
        assert_eq!(quota.bytes_per_account, 256 * 1024 * 1024);
        assert_eq!(quota.bundles_per_account, 5000);
        assert_eq!(quota.max_stored_bytes, None);
        assert_eq!(QuotaKind::Bytes.as_str(), "bytes");
        assert_eq!(QuotaKind::Bundles.as_str(), "bundles");
    }
}
