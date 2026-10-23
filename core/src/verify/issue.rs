//! A single located problem found by a check.

use serde::{Deserialize, Serialize};

use crate::error::{CrispError, ErrorCode};

/// One problem a check found, located by a JSON Pointer into the payload.
///
/// The verifier condenses a check's issues into one report line; editor
/// validation ([`crate::author::validate_draft`]) returns them all so a UI
/// can mark the offending fields.
#[cfg_attr(feature = "ts", derive(tsify::Tsify), tsify(hashmap_as_object))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// A JSON Pointer (RFC 6901) into the payload, e.g.
    /// `/recipes/0/steps/2/timers/0/duration`. Empty for the whole document.
    pub path: String,
    /// The stable error code.
    #[cfg_attr(feature = "ts", tsify(type = "string"))]
    pub code: ErrorCode,
    /// A human-readable explanation.
    pub message: String,
}

impl Issue {
    /// Builds an issue.
    #[must_use]
    pub fn new(path: impl Into<String>, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            code,
            message: message.into(),
        }
    }

    /// Builds an issue from an error raised at `path`. Operational errors
    /// without a stable code are reported as [`ErrorCode::Schema`].
    #[must_use]
    pub fn from_error(path: impl Into<String>, error: &CrispError) -> Self {
        let message = match error {
            CrispError::Check { detail, .. } => detail.clone(),
            other => other.to_string(),
        };
        Self::new(path, error.code().unwrap_or(ErrorCode::Schema), message)
    }

    /// Converts this issue into a [`CrispError`] for a report line.
    #[must_use]
    pub fn to_error(&self) -> CrispError {
        let detail = if self.path.is_empty() {
            self.message.clone()
        } else {
            format!("{} (at {})", self.message, self.path)
        };
        CrispError::check(self.code, detail)
    }
}

/// Collects [`Issue`]s for one check.
#[derive(Debug, Default)]
pub(crate) struct Issues(pub(crate) Vec<Issue>);

impl Issues {
    /// Records an issue.
    pub(crate) fn push(&mut self, path: &str, code: ErrorCode, message: impl Into<String>) {
        self.0.push(Issue::new(path, code, message));
    }

    /// Records an issue if `ok` is false.
    pub(crate) fn require(
        &mut self,
        ok: bool,
        path: &str,
        code: ErrorCode,
        message: impl FnOnce() -> String,
    ) {
        if !ok {
            self.push(path, code, message());
        }
    }
}
