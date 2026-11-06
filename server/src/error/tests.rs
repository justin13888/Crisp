use super::*;
use crate::quota::QuotaKind;
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
        (Forbidden::new("x").into_problem(), code::FORBIDDEN, 403),
        (
            ReauthenticationRequired::new(600).into_problem(),
            code::REAUTHENTICATION_REQUIRED,
            403,
        ),
        (
            TooManyRequests::new("x", std::time::Duration::ZERO).into_problem(),
            code::RATE_LIMITED,
            429,
        ),
        (
            QuotaExceeded::new(QuotaKind::Bytes, 1, 1, 1).into_problem(),
            code::QUOTA_EXCEEDED,
            403,
        ),
        (
            StorageFull::new(1, 1, 1).into_problem(),
            code::STORAGE_FULL,
            507,
        ),
        (Removed::new("spam").into_problem(), code::REMOVED, 410),
        (
            RemovedConflict::new("spam").into_problem(),
            code::REMOVED,
            409,
        ),
        (
            AccountSuspended::new().into_problem(),
            code::ACCOUNT_SUSPENDED,
            403,
        ),
        (
            Internal::logged("test", &"boom").into_problem(),
            code::INTERNAL,
            500,
        ),
        (
            ServiceUnavailable::logged("test", &"down").into_problem(),
            code::UNAVAILABLE,
            503,
        ),
    ];
    for (problem, expected, status) in cases {
        assert_eq!(problem.status.as_u16(), status);
        assert_eq!(problem.extensions["code"], expected);
        assert!(problem.type_uri.starts_with(PROBLEM_BASE));
    }
}

#[test]
fn quota_exceeded_reports_the_quota_and_the_numbers() {
    let problem = QuotaExceeded::new(QuotaKind::Bundles, 5000, 5000, 1).into_problem();
    assert_eq!(problem.type_uri, "urn:crisp:problem:quota-exceeded");
    assert_eq!(problem.extensions["quota"], "bundles");
    assert_eq!(problem.extensions["limit"], 5000);
    assert_eq!(problem.extensions["used"], 5000);
    assert_eq!(problem.extensions["requested"], 1);
    assert!(problem.detail.unwrap_or_default().contains("bundle quota"));
}

#[test]
fn both_removed_refusals_share_one_type_and_carry_the_reason() {
    for problem in [
        Removed::new("copyright").into_problem(),
        RemovedConflict::new("copyright").into_problem(),
    ] {
        assert_eq!(problem.type_uri, "urn:crisp:problem:removed");
        assert_eq!(problem.extensions["reason"], "copyright");
    }
    let problem = AccountSuspended::new().into_problem();
    assert_eq!(problem.type_uri, "urn:crisp:problem:account-suspended");
}

#[test]
fn storage_full_reports_no_numbers() {
    let problem = StorageFull::new(1000, 990, 11).into_problem();
    assert_eq!(problem.type_uri, "urn:crisp:problem:storage-full");
    assert_eq!(problem.status.as_u16(), 507);
    assert!(!problem.extensions.contains_key("limit"));
    assert!(!problem.extensions.contains_key("used"));
}

#[test]
fn retry_after_rounds_up_to_whole_seconds() {
    use std::time::Duration;
    let secs = |wait| TooManyRequests::new("x", wait).retry_after_secs();
    assert_eq!(secs(Duration::ZERO), 1);
    assert_eq!(secs(Duration::from_millis(1)), 1);
    assert_eq!(secs(Duration::from_secs(1)), 1);
    assert_eq!(secs(Duration::from_millis(1500)), 2);
    assert_eq!(secs(Duration::from_secs(20)), 20);
}

#[test]
fn internal_errors_do_not_leak_details() {
    let problem = Internal::logged("test", &"secret connection string").into_problem();
    assert_eq!(problem.detail.as_deref(), Some("internal server error"));
    let problem = ServiceUnavailable::logged("test", &"secret connection string").into_problem();
    assert!(!problem.detail.unwrap_or_default().contains("secret"));
}

#[test]
fn database_failures_sort_into_unavailable_or_internal() {
    use sea_orm::{ConnAcquireErr, DbErr};
    let down = DbFailure::from(DbErr::ConnectionAcquire(ConnAcquireErr::Timeout));
    assert!(matches!(down, DbFailure::Unavailable(_)));
    let broken = DbFailure::from(DbErr::Custom("bad".to_owned()));
    assert!(matches!(broken, DbFailure::Internal(_)));
}
