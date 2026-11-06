//! Turning a documented error response into a JS failure: its RFC 9457
//! `detail` becomes the message, and a Crisp problem `type` becomes the
//! failure's `code`.

use serde::Serialize;

use crate::api::{self, self as support};

/// The `type` URI prefix every Crisp problem shares.
const PROBLEM_BASE: &str = "urn:crisp:problem:";

/// Reads the RFC 9457 members of a documented error body.
pub(super) trait Problem {
    /// The body as JSON, if it carries one.
    fn body(&self) -> Option<serde_json::Value>;
}

fn body_of<T: Serialize>(body: &T) -> Option<serde_json::Value> {
    serde_json::to_value(body).ok()
}

macro_rules! problems {
    ($($error:ident { $($variant:ident),* $(,)? }),* $(,)?) => {
        $(impl Problem for api::$error {
            fn body(&self) -> Option<serde_json::Value> {
                match self {
                    $(api::$error::$variant(body) => body_of(body),)*
                }
            }
        })*
    };
}

problems! {
    AccountProfileError { Status400, Status404, Status500, Status503 },
    AccountUsageError { Status401, Status403, Status500, Status503 },
    ChallengeError { Status400, Status413, Status415, Status422, Status429, Status500, Status503 },
    LoginError { Status400, Status401, Status413, Status415, Status422, Status429, Status500, Status503 },
    LogoutError { Status401, Status403, Status500, Status503 },
    LogoutAllError { Status401, Status403, Status500, Status503 },
    RegisterError { Status400, Status409, Status413, Status415, Status422, Status429, Status500, Status503 },
    PublishError { Status400, Status401, Status403, Status409, Status413, Status415, Status422, Status429, Status500, Status503, Status507 },
    GetLatestError { Status400, Status404, Status500, Status503 },
    GetExactError { Status400, Status404, Status500, Status503 },
    UnpublishDocumentError { Status400, Status401, Status403, Status404, Status500, Status503 },
    UnpublishRevisionError { Status400, Status401, Status403, Status404, Status500, Status503 },
    DeleteAccountError { Status401, Status403, Status500, Status503 },
    BrowseError { Status400, Status429, Status500, Status503 },
    SearchError { Status400, Status429, Status500, Status503 },
}

/// `GET /server-info` documents one error, the 500 a recovered panic
/// becomes, so its error is the problem body itself rather than an enum.
impl Problem for api::ServerInfoError {
    fn body(&self) -> Option<serde_json::Value> {
        body_of(&self.0)
    }
}

/// The problem name in a Crisp problem `type` (`quota-exceeded` for
/// `urn:crisp:problem:quota-exceeded`), or `None` for any other type.
pub(super) fn problem_code(problem_type: &str) -> Option<&str> {
    problem_type
        .strip_prefix(PROBLEM_BASE)
        .filter(|name| !name.is_empty())
}

/// What a JS failure for a client error carries.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Failure {
    /// The server's problem `detail`, or a sentence naming what failed.
    pub(super) message: String,
    /// The Crisp problem name of the response's `type`.
    pub(super) code: Option<String>,
    /// The HTTP status, when a response arrived.
    pub(super) status: Option<u16>,
}

/// Converts a client error into a JS failure with `status`, the server's
/// problem `detail` as its message, and the Crisp problem name as its
/// `code`. A macro so the JS conversion, which only a JS host can run,
/// stays at the call site; [`describe`] is everything it decides.
macro_rules! server_failure {
    ($what:expr, $error:expr) => {{
        let described = problem::describe($what, $error);
        failure(
            &described.message,
            described.code.as_deref(),
            described.status,
        )
    }};
}
pub(super) use server_failure;

/// A lookup's response body, `None` when the server answered 404, or the
/// error for any other failure. The error is the generated client's own,
/// passed through unchanged, so its size is not this function's to choose.
#[allow(clippy::result_large_err)]
pub(super) fn found<T, E>(
    result: Result<api::ResponseValue<T>, support::Error<E>>,
) -> Result<Option<T>, support::Error<E>> {
    match result {
        Ok(response) => Ok(Some(response.into_inner())),
        Err(e) if e.status().is_some_and(|s| s.as_u16() == 404) => Ok(None),
        Err(e) => Err(e),
    }
}

/// What the JS failure for `error`, raised while `what`, carries.
pub(super) fn describe<E: Problem + std::fmt::Debug + std::fmt::Display>(
    what: &str,
    error: &support::Error<E>,
) -> Failure {
    let status = error.status().map(|s| s.as_u16());
    let body = match error {
        support::Error::Api(value) => value.inner().body(),
        _ => None,
    };
    let member = |name: &str| {
        body.as_ref()
            .and_then(|body| body.get(name)?.as_str().map(str::to_owned))
    };
    let code = member("type");
    let code = code.as_deref().and_then(problem_code).map(str::to_owned);
    let message = match (member("detail"), status) {
        (Some(detail), _) => detail,
        (None, Some(status)) => format!("{what} failed with HTTP {status}"),
        (None, None) if error.is_transient() => {
            format!("{what} failed: the server could not be reached")
        }
        (None, None) => format!("{what} failed: {error}"),
    };
    Failure {
        message,
        code,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::{Failure, Problem, describe, found, problem_code};
    use crate::api::{self, self as support};

    fn api_error(status: u16, body: serde_json::Value) -> support::Error<api::PublishError> {
        let problem = serde_json::from_value(body).expect("a problem body");
        support::Error::Api(support::ResponseValue::new(
            reqwest::StatusCode::from_u16(status).expect("a status"),
            reqwest::header::HeaderMap::new(),
            api::PublishError::Status403(Box::new(problem)),
        ))
    }

    #[test]
    fn a_documented_body_reads_as_json() {
        let error = api::PublishError::Status403(Box::new(
            serde_json::from_value(serde_json::json!({
                "type": "urn:crisp:problem:quota-exceeded",
                "status": 403,
            }))
            .expect("a problem body"),
        ));
        let body = error.body().expect("a body");
        assert_eq!(body["type"], "urn:crisp:problem:quota-exceeded");
        assert_eq!(body["status"], 403);
    }

    #[test]
    fn a_server_info_failure_reads_its_problem_body() {
        let error = api::ServerInfoError(
            serde_json::from_value(serde_json::json!({ "type": "about:blank", "status": 500 }))
                .expect("a problem body"),
        );
        let body = error.body().expect("a body");
        assert_eq!(body["status"], 500);
    }

    #[test]
    fn a_problem_response_carries_its_detail_code_and_status() {
        let error = api_error(
            403,
            serde_json::json!({
                "type": "urn:crisp:problem:quota-exceeded",
                "status": 403,
                "detail": "over quota",
            }),
        );
        assert_eq!(
            describe("publishing", &error),
            Failure {
                message: "over quota".to_owned(),
                code: Some("quota-exceeded".to_owned()),
                status: Some(403),
            }
        );
    }

    #[test]
    fn a_response_without_detail_names_its_status() {
        let error = api_error(
            403,
            serde_json::json!({ "type": "about:blank", "status": 403 }),
        );
        assert_eq!(
            describe("publishing", &error),
            Failure {
                message: "publishing failed with HTTP 403".to_owned(),
                code: None,
                status: Some(403),
            }
        );
    }

    #[test]
    fn a_lookup_reads_404_as_absent_and_keeps_other_failures() {
        let ok: Result<_, support::Error<api::PublishError>> = Ok(support::ResponseValue::new(
            reqwest::StatusCode::OK,
            reqwest::header::HeaderMap::new(),
            7,
        ));
        assert_eq!(found(ok).expect("found"), Some(7));
        let missing = api_error(
            404,
            serde_json::json!({ "type": "about:blank", "status": 404 }),
        );
        assert_eq!(found::<u8, _>(Err(missing)).expect("absent"), None);
        let refused = api_error(
            403,
            serde_json::json!({ "type": "about:blank", "status": 403 }),
        );
        let kept = found::<u8, _>(Err(refused)).expect_err("a failure");
        assert_eq!(kept.status().map(|s| s.as_u16()), Some(403));
        let offline = support::Error::<api::PublishError>::Timeout(support::TimeoutKind::Connect);
        assert!(found::<u8, _>(Err(offline)).is_err());
    }

    #[test]
    fn an_unreachable_server_is_named_as_such() {
        let error = support::Error::<api::PublishError>::Timeout(support::TimeoutKind::Connect);
        assert!(error.is_transient());
        assert_eq!(
            describe("publishing", &error).message,
            "publishing failed: the server could not be reached"
        );
    }

    #[test]
    fn any_other_failure_carries_its_own_text() {
        let error = support::Error::<api::PublishError>::RequestConstruction(
            support::RequestError::MissingCredential {
                alternatives: vec![vec!["bearer"]],
            },
        );
        assert!(!error.is_transient());
        let failure = describe("publishing", &error);
        assert_eq!(failure.message, format!("publishing failed: {error}"));
        assert_eq!((failure.code, failure.status), (None, None));
    }

    #[test]
    fn crisp_problem_types_name_their_code() {
        assert_eq!(
            problem_code("urn:crisp:problem:quota-exceeded"),
            Some("quota-exceeded")
        );
        assert_eq!(
            problem_code("urn:crisp:problem:storage-full"),
            Some("storage-full")
        );
        assert_eq!(problem_code("urn:crisp:problem:"), None);
        assert_eq!(problem_code("about:blank"), None);
    }
}
