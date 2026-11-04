//! Turning a documented error response into a JS failure: its RFC 9457
//! `detail` becomes the message, and a Crisp problem `type` becomes the
//! failure's `code`.

use serde::Serialize;
use wasm_bindgen::JsValue;

use crate::api::{self, self as support};
use crate::boundary::failure;

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

impl Problem for std::convert::Infallible {
    fn body(&self) -> Option<serde_json::Value> {
        match *self {}
    }
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
    RegisterError { Status400, Status409, Status413, Status415, Status422, Status429, Status500, Status503 },
    PublishError { Status400, Status401, Status403, Status409, Status413, Status415, Status422, Status429, Status500, Status503, Status507 },
    GetLatestError { Status400, Status404, Status500, Status503 },
    GetExactError { Status400, Status404, Status500, Status503 },
    BrowseError { Status400, Status429, Status500, Status503 },
    SearchError { Status400, Status429, Status500, Status503 },
}

/// The problem name in a Crisp problem `type` (`quota-exceeded` for
/// `urn:crisp:problem:quota-exceeded`), or `None` for any other type.
pub(super) fn problem_code(problem_type: &str) -> Option<&str> {
    problem_type
        .strip_prefix(PROBLEM_BASE)
        .filter(|name| !name.is_empty())
}

/// Converts a client error into a JS failure with `status`, the server's
/// problem `detail` as its message, and the Crisp problem name as its
/// `code`.
pub(super) fn server_failure<E: Problem + std::fmt::Debug + std::fmt::Display>(
    what: &str,
    error: &support::Error<E>,
) -> JsValue {
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
    let code = code.as_deref().and_then(problem_code);
    let message = match (member("detail"), status) {
        (Some(detail), _) => detail,
        (None, Some(status)) => format!("{what} failed with HTTP {status}"),
        (None, None) if error.is_transient() => {
            format!("{what} failed: the server could not be reached")
        }
        (None, None) => format!("{what} failed: {error}"),
    };
    failure(&message, code, status)
}

#[cfg(test)]
mod tests {
    use super::problem_code;

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
