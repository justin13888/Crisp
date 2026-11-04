//! The API error vocabulary and its RFC 9457 projection.
//!
//! Every failure a handler can produce is one of fifteen *kinds*, each a struct
//! deriving [`kynos::ApiError`] with a fixed status and a stable,
//! machine-readable `code` extension member:
//!
//! | kind                         | status | `code`                      |
//! | ---------------------------- | ------ | --------------------------- |
//! | [`NotFound`]                 | 404    | `not_found`                 |
//! | [`BadRequest`]               | 400    | `bad_request`               |
//! | [`Unauthorized`]             | 401    | `unauthorized`              |
//! | [`Forbidden`]                | 403    | `forbidden`                 |
//! | [`ReauthenticationRequired`] | 403    | `reauthentication_required` |
//! | [`QuotaExceeded`]            | 403    | `quota_exceeded`            |
//! | [`AccountSuspended`]         | 403    | `account_suspended`         |
//! | [`Conflict`]                 | 409    | `conflict`                  |
//! | [`RemovedConflict`]          | 409    | `removed`                   |
//! | [`Removed`]                  | 410    | `removed`                   |
//! | [`VerificationFailed`]       | 422    | `verification_failed`       |
//! | [`TooManyRequests`]          | 429    | `rate_limited`              |
//! | [`Internal`]                 | 500    | `internal`                  |
//! | [`ServiceUnavailable`]       | 503    | `unavailable`               |
//! | [`StorageFull`]              | 507    | `storage_full`              |
//!
//! Each operation then names exactly the kinds it can produce in its own
//! error enum (built with [`operation_error!`](crate::operation_error)), so
//! the OpenAPI document advertises only the statuses that operation can
//! really answer with — a contract a client generator can trust.
//!
//! Bodies are `application/problem+json`; `detail` carries the
//! occurrence-specific sentence, and a verification failure additionally
//! carries the full compliance `report`.

use crisp_core::verify::ComplianceReport;
use kynos::ApiError;

mod moderation;
mod quota;

pub use moderation::{AccountSuspended, ReauthenticationRequired, Removed, RemovedConflict};
pub use quota::{QuotaExceeded, StorageFull};

/// The `type` URI prefix every Crisp problem shares.
pub const PROBLEM_BASE: &str = "urn:crisp:problem:";

/// The stable `code` extension values, one per error kind.
pub mod code {
    /// The requested resource does not exist.
    pub const NOT_FOUND: &str = "not_found";
    /// The request was malformed or semantically invalid.
    pub const BAD_REQUEST: &str = "bad_request";
    /// Authentication failed.
    pub const UNAUTHORIZED: &str = "unauthorized";
    /// The caller may not act on the resource: it belongs to another account.
    pub const FORBIDDEN: &str = "forbidden";
    /// The operation needs a session newer than the caller's; sign in again.
    pub const REAUTHENTICATION_REQUIRED: &str = "reauthentication_required";
    /// The request conflicts with existing state.
    pub const CONFLICT: &str = "conflict";
    /// A bundle failed the A–E verification pipeline.
    pub const VERIFICATION_FAILED: &str = "verification_failed";
    /// An operation's rate-limit budget is exhausted.
    pub const RATE_LIMITED: &str = "rate_limited";
    /// A publish would exceed one of the account's storage quotas.
    pub const QUOTA_EXCEEDED: &str = "quota_exceeded";
    /// A publish would exceed the server's total storage cap.
    pub const STORAGE_FULL: &str = "storage_full";
    /// An administrator took the document or revision down.
    pub const REMOVED: &str = "removed";
    /// The account is suspended: it cannot sign in or register again.
    pub const ACCOUNT_SUSPENDED: &str = "account_suspended";
    /// An unexpected server-side failure.
    pub const INTERNAL: &str = "internal";
    /// The database is unreachable or overloaded; retry later.
    pub const UNAVAILABLE: &str = "unavailable";
}

/// The requested resource does not exist.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{detail}")]
#[problem(base = PROBLEM_BASE, status = 404, title = "Not found")]
pub struct NotFound {
    /// What was not found.
    detail: String,
    /// Always [`code::NOT_FOUND`].
    #[problem(extension)]
    code: &'static str,
}

impl NotFound {
    /// A not-found failure with an explanation.
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
            code: code::NOT_FOUND,
        }
    }
}

/// The request was malformed or semantically invalid.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{detail}")]
#[problem(base = PROBLEM_BASE, status = 400, title = "Bad request")]
pub struct BadRequest {
    /// What was wrong with the request.
    detail: String,
    /// Always [`code::BAD_REQUEST`].
    #[problem(extension)]
    code: &'static str,
}

impl BadRequest {
    /// A bad-request failure with an explanation.
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
            code: code::BAD_REQUEST,
        }
    }
}

impl From<crisp_core::CrispError> for BadRequest {
    fn from(error: crisp_core::CrispError) -> Self {
        Self::new(error.to_string())
    }
}

/// Authentication failed (a login handshake that did not verify).
///
/// Bearer-token failures on protected routes are answered by kynos's own
/// `Auth` rejection instead, which also sends `WWW-Authenticate`.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{detail}")]
#[problem(base = PROBLEM_BASE, status = 401, title = "Unauthorized")]
pub struct Unauthorized {
    /// Why authentication failed, phrased so it leaks nothing exploitable.
    detail: String,
    /// Always [`code::UNAUTHORIZED`].
    #[problem(extension)]
    code: &'static str,
}

impl Unauthorized {
    /// An authentication failure with an explanation.
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
            code: code::UNAUTHORIZED,
        }
    }
}

/// The caller is authenticated but may not act on the resource, because
/// another account owns it (e.g. a co-author's revision).
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{detail}")]
#[problem(base = PROBLEM_BASE, status = 403, title = "Forbidden")]
pub struct Forbidden {
    /// Why the caller may not act on the resource.
    detail: String,
    /// Always [`code::FORBIDDEN`].
    #[problem(extension)]
    code: &'static str,
}

impl Forbidden {
    /// A refusal with an explanation.
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
            code: code::FORBIDDEN,
        }
    }
}

/// The request conflicts with existing state (e.g. a duplicate).
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{detail}")]
#[problem(base = PROBLEM_BASE, status = 409, title = "Conflict")]
pub struct Conflict {
    /// What the request conflicts with.
    detail: String,
    /// Always [`code::CONFLICT`].
    #[problem(extension)]
    code: &'static str,
}

impl Conflict {
    /// A conflict failure with an explanation.
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
            code: code::CONFLICT,
        }
    }
}

/// A compliance report, as published in a [`VerificationFailed`] problem.
///
/// Wraps `crisp-core`'s report so it can be rendered as a JSON extension
/// member without the core crate knowing about HTTP.
#[derive(Debug)]
pub struct ReportExtension(Box<ComplianceReport>);

impl From<ReportExtension> for serde_json::Value {
    fn from(report: ReportExtension) -> Self {
        serde_json::to_value(&*report.0).unwrap_or_else(|error| {
            tracing::error!(%error, "failed to serialize a compliance report");
            Self::Null
        })
    }
}

/// A submitted bundle failed the A–E verification pipeline.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{detail}")]
#[problem(base = PROBLEM_BASE, status = 422, title = "Verification failed")]
pub struct VerificationFailed {
    /// A one-line summary of the first failed check.
    detail: String,
    /// Always [`code::VERIFICATION_FAILED`].
    #[problem(extension)]
    code: &'static str,
    /// The full `crisp-core` compliance report.
    #[problem(extension)]
    report: ReportExtension,
}

impl VerificationFailed {
    /// A verification failure carrying its compliance report.
    #[must_use]
    pub fn new(report: ComplianceReport) -> Self {
        let detail = report.failures().first().map_or_else(
            || "bundle failed verification".to_owned(),
            |check| {
                format!(
                    "bundle failed verification at check {}: {}",
                    check.id,
                    check.detail.as_deref().unwrap_or("no detail")
                )
            },
        );
        Self {
            detail,
            code: code::VERIFICATION_FAILED,
            report: ReportExtension(Box::new(report)),
        }
    }
}

/// The `Retry-After` header a [`TooManyRequests`] answer carries.
#[derive(Debug, kynos::HeaderParams)]
pub struct RetryAfter {
    /// Whole seconds until the request would be allowed (at least 1).
    #[header(rename = "Retry-After")]
    retry_after: u64,
}

/// The caller exhausted an operation's rate-limit budget (see
/// [`crate::limits`]).
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{detail}")]
#[problem(base = PROBLEM_BASE, status = 429, title = "Too many requests")]
pub struct TooManyRequests {
    /// Which budget was exhausted and when to retry.
    detail: String,
    /// Always [`code::RATE_LIMITED`].
    #[problem(extension)]
    code: &'static str,
    /// When the client may retry.
    #[problem(headers)]
    retry: RetryAfter,
}

impl TooManyRequests {
    /// A refusal of `operation` for `wait`, rounded up to whole seconds and
    /// never less than one.
    #[must_use]
    pub fn new(operation: &str, wait: std::time::Duration) -> Self {
        let mut retry_after = wait.as_secs();
        if wait.subsec_nanos() > 0 || retry_after == 0 {
            retry_after += 1;
        }
        Self {
            detail: format!("the {operation} rate limit is exhausted; retry in {retry_after}s"),
            code: code::RATE_LIMITED,
            retry: RetryAfter { retry_after },
        }
    }

    /// The `Retry-After` value, in whole seconds.
    #[must_use]
    pub const fn retry_after_secs(&self) -> u64 {
        self.retry.retry_after
    }
}

/// An unexpected internal failure; details are logged, never exposed.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("internal server error")]
#[problem(base = PROBLEM_BASE, status = 500, title = "Internal server error")]
pub struct Internal {
    /// Always [`code::INTERNAL`].
    #[problem(extension)]
    code: &'static str,
}

impl Internal {
    /// Logs `error` at ERROR level and returns the opaque 500.
    #[must_use]
    pub fn logged(context: &'static str, error: &dyn std::fmt::Display) -> Self {
        tracing::error!(%error, context, "internal error");
        Self {
            code: code::INTERNAL,
        }
    }
}

impl From<sea_orm::DbErr> for Internal {
    fn from(error: sea_orm::DbErr) -> Self {
        Self::logged("database", &error)
    }
}

/// The database is unreachable, overloaded, or too slow to answer; the
/// client should retry later. Details are logged, never exposed.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("the service is temporarily unavailable; retry later")]
#[problem(base = PROBLEM_BASE, status = 503, title = "Service unavailable")]
pub struct ServiceUnavailable {
    /// Always [`code::UNAVAILABLE`].
    #[problem(extension)]
    code: &'static str,
}

impl ServiceUnavailable {
    /// Logs `error` at ERROR level and returns the opaque 503.
    #[must_use]
    pub fn logged(context: &'static str, error: &dyn std::fmt::Display) -> Self {
        tracing::error!(%error, context, "service unavailable");
        Self {
            code: code::UNAVAILABLE,
        }
    }
}

/// A database failure sorted into the answer a client can act on: retry
/// later (503) when the database could not be reached or could not answer in
/// time (see [`crate::storage::is_unavailable`]), otherwise a 500.
#[derive(Debug)]
pub enum DbFailure {
    /// The database is unavailable.
    Unavailable(ServiceUnavailable),
    /// Any other database failure.
    Internal(Internal),
}

impl From<sea_orm::DbErr> for DbFailure {
    fn from(error: sea_orm::DbErr) -> Self {
        if crate::storage::is_unavailable(&error) {
            Self::Unavailable(ServiceUnavailable::logged("database", &error))
        } else {
            Self::Internal(Internal::from(error))
        }
    }
}

/// Declares one operation's error enum: the listed error kinds plus
/// [`Internal`] and [`ServiceUnavailable`], each a transparent variant, so
/// the operation's documented responses are exactly the union of those
/// kinds' statuses. Every such operation reaches the database, so every one
/// can answer 503.
///
/// ```ignore
/// operation_error! {
///     /// What `POST /auth/login` can fail with.
///     pub enum LoginError { BadRequest, Unauthorized }
/// }
/// ```
///
/// A database error converts through [`DbFailure`] into the
/// `ServiceUnavailable` or `Internal` variant, so `?` works on storage calls.
#[macro_export]
macro_rules! operation_error {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident { $($kind:ident),* $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, thiserror::Error, kynos::ApiError)]
        $vis enum $name {
            $(
                #[doc = concat!("See [`", stringify!($kind), "`](crate::error::", stringify!($kind), ").")]
                #[error(transparent)]
                #[problem(transparent)]
                $kind(#[from] $crate::error::$kind),
            )*
            /// See [`Internal`](crate::error::Internal).
            #[error(transparent)]
            #[problem(transparent)]
            Internal(#[from] $crate::error::Internal),
            /// See [`ServiceUnavailable`](crate::error::ServiceUnavailable).
            #[error(transparent)]
            #[problem(transparent)]
            ServiceUnavailable(#[from] $crate::error::ServiceUnavailable),
        }

        impl From<$crate::error::DbFailure> for $name {
            fn from(failure: $crate::error::DbFailure) -> Self {
                match failure {
                    $crate::error::DbFailure::Unavailable(e) => Self::ServiceUnavailable(e),
                    $crate::error::DbFailure::Internal(e) => Self::Internal(e),
                }
            }
        }

        impl From<sea_orm::DbErr> for $name {
            fn from(error: sea_orm::DbErr) -> Self {
                $crate::error::DbFailure::from(error).into()
            }
        }
    };
}

#[cfg(test)]
mod tests;
