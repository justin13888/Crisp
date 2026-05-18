//! The machine-readable compliance report (`crisp-v1.cddl`:
//! `compliance-report`).

use serde::{Deserialize, Serialize};

use crate::model::primitives::Timestamp;

/// The marker value of a compliance report's `crisp_report` field.
pub const CRISP_REPORT_MARKER: u8 = 1;

/// The outcome of a single verification check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// The check passed.
    Pass,
    /// The check failed.
    Fail,
    /// The check was not run (a fatal earlier failure, or it did not apply).
    Skipped,
}

/// The result of one verification check (`crisp-v1.cddl`: `check-result`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckResult {
    /// The check id, e.g. `"A1"` or `"C3"`.
    pub id: String,
    /// Whether the check passed, failed, or was skipped.
    pub status: CheckStatus,
    /// The stable error code, present on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// A human-readable explanation, present on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The compliance class a bundle achieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComplianceClass {
    /// Phases A–D passed (no cryptographic guarantee).
    Structural,
    /// Phases A–E passed, including author signature(s) and one endorsement.
    Published,
}

/// How much trust a verified bundle carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Trust {
    /// No valid signatures.
    Unsigned,
    /// At least one valid author signature.
    AuthorSigned,
    /// A valid author signature and a valid server endorsement.
    Endorsed,
}

/// A complete verification verdict (`crisp-v1.cddl`: `compliance-report`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComplianceReport {
    /// Always [`CRISP_REPORT_MARKER`].
    pub crisp_report: u8,
    /// Whether the bundle is compliant for its [`ComplianceClass`].
    pub compliant: bool,
    /// The class the report is about.
    pub class: ComplianceClass,
    /// The bundle's declared format version (`0` if it could not be read).
    pub format_version: u64,
    /// The trust level established.
    pub trust: Trust,
    /// When verification was performed.
    pub verified_at: Timestamp,
    /// Every check, in order.
    pub checks: Vec<CheckResult>,
}

impl ComplianceReport {
    /// Returns the failed checks.
    #[must_use]
    pub fn failures(&self) -> Vec<&CheckResult> {
        self.checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .collect()
    }
}
