//! The API error vocabulary and its RFC 9457 projection.
//!
//! Every failure a handler can produce is one of six *kinds*, each a struct
//! deriving [`kynos::ApiError`] with a fixed status and a stable,
//! machine-readable `code` extension member:
//!
//! | kind                   | status | `code`                |
//! |------------------------|--------|-----------------------|
//! | [`NotFound`]           | 404    | `not_found`           |
//! | [`BadRequest`]         | 400    | `bad_request`         |
//! | [`Unauthorized`]       | 401    | `unauthorized`        |
//! | [`Conflict`]           | 409    | `conflict`            |
//! | [`VerificationFailed`] | 422    | `verification_failed` |
//! | [`Internal`]           | 500    | `internal`            |
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
    /// The request conflicts with existing state.
    pub const CONFLICT: &str = "conflict";
    /// A bundle failed the A–E verification pipeline.
    pub const VERIFICATION_FAILED: &str = "verification_failed";
    /// An unexpected server-side failure.
    pub const INTERNAL: &str = "internal";
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

/// Declares one operation's error enum: the listed error kinds plus
/// [`Internal`], each a transparent variant, so the operation's documented
/// responses are exactly the union of those kinds' statuses.
///
/// ```ignore
/// operation_error! {
///     /// What `POST /auth/login` can fail with.
///     pub enum LoginError { BadRequest, Unauthorized }
/// }
/// ```
///
/// A database error converts into the `Internal` variant, so `?` works on
/// storage calls.
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
        }

        impl From<sea_orm::DbErr> for $name {
            fn from(error: sea_orm::DbErr) -> Self {
                Self::Internal($crate::error::Internal::from(error))
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use kynos::error::problem::IntoProblem;

    #[test]
    fn every_kind_carries_its_stable_code() {
        let cases: Vec<(kynos::Problem, &str, u16)> = vec![
            (NotFound::new("x").into_problem(), code::NOT_FOUND, 404),
            (BadRequest::new("x").into_problem(), code::BAD_REQUEST, 400),
            (
                Unauthorized::new("x").into_problem(),
                code::UNAUTHORIZED,
                401,
            ),
            (Conflict::new("x").into_problem(), code::CONFLICT, 409),
            (
                Internal::logged("test", &"boom").into_problem(),
                code::INTERNAL,
                500,
            ),
        ];
        for (problem, expected, status) in cases {
            assert_eq!(problem.status.as_u16(), status);
            assert_eq!(problem.extensions["code"], expected);
            assert!(problem.type_uri.starts_with(PROBLEM_BASE));
        }
    }

    #[test]
    fn internal_errors_do_not_leak_details() {
        let problem = Internal::logged("test", &"secret connection string").into_problem();
        assert_eq!(problem.detail.as_deref(), Some("internal server error"));
    }
}
