//! The account and moderation refusals: a session too old for the operation,
//! a taken-down document (410 on reads, 409 on a publish), and a suspended
//! account (403 on sign-in and registration).

use kynos::ApiError;

use super::{PROBLEM_BASE, ProblemCode, code};

/// The operation needs a recent sign-in and the caller's session is older:
/// the client signs in again (spec §8.7) and retries with the new session.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{detail}")]
#[problem(base = PROBLEM_BASE, status = 403, title = "Reauthentication required")]
pub struct ReauthenticationRequired {
    /// How recent the session must be.
    detail: String,
    /// Always [`code::REAUTHENTICATION_REQUIRED`].
    #[problem(extension)]
    code: ProblemCode,
}

impl ReauthenticationRequired {
    /// A refusal naming the largest session age, in seconds, the operation
    /// accepts.
    #[must_use]
    pub fn new(max_age_secs: i64) -> Self {
        Self {
            detail: format!(
                "this operation needs a session at most {max_age_secs}s old; sign in again"
            ),
            code: ProblemCode(code::REAUTHENTICATION_REQUIRED),
        }
    }
}

/// An administrator took the document, or this revision of it, down: the
/// server no longer offers it (`docs/public-server-policy.md` §4.3).
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{detail}")]
#[problem(base = PROBLEM_BASE, status = 410, title = "Removed")]
pub struct Removed {
    /// What was removed and why.
    detail: String,
    /// Always [`code::REMOVED`].
    #[problem(extension)]
    code: ProblemCode,
    /// The takedown's reason category, e.g. `copyright`.
    #[problem(extension)]
    reason: String,
}

impl Removed {
    /// A refusal to serve something taken down for `reason`.
    #[must_use]
    pub fn new(reason: &str) -> Self {
        Self {
            detail: format!("this document was removed by the server operator ({reason})"),
            code: ProblemCode(code::REMOVED),
            reason: reason.to_owned(),
        }
    }
}

/// A publish names a document, or a revision, that an administrator took
/// down; it stays refused until the takedown is lifted.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{detail}")]
#[problem(status = 409, title = "Removed", type = "urn:crisp:problem:removed")]
pub struct RemovedConflict {
    /// What was removed and why.
    detail: String,
    /// Always [`code::REMOVED`].
    #[problem(extension)]
    code: ProblemCode,
    /// The takedown's reason category.
    #[problem(extension)]
    reason: String,
}

impl RemovedConflict {
    /// A refusal to publish something taken down for `reason`.
    #[must_use]
    pub fn new(reason: &str) -> Self {
        Self {
            detail: format!(
                "this document was removed by the server operator ({reason}) and cannot be \
                 published again"
            ),
            code: ProblemCode(code::REMOVED),
            reason: reason.to_owned(),
        }
    }
}

/// The account is suspended: it cannot sign in, and its key cannot register
/// again (`docs/public-server-policy.md` §4.4).
#[derive(Debug, thiserror::Error, ApiError)]
#[error("this account is suspended")]
#[problem(base = PROBLEM_BASE, status = 403, title = "Account suspended")]
pub struct AccountSuspended {
    /// Always [`code::ACCOUNT_SUSPENDED`].
    #[problem(extension)]
    code: ProblemCode,
}

impl AccountSuspended {
    /// The suspended-account refusal.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            code: ProblemCode(code::ACCOUNT_SUSPENDED),
        }
    }
}

impl Default for AccountSuspended {
    fn default() -> Self {
        Self::new()
    }
}
