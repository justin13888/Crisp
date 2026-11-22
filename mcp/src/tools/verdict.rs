//! The offline A–E verdict every file the tools trust must pass: an edit
//! base, a file `get_recipe` reports as verified, and the endorsed copy
//! `publish_recipe` writes back.

use crisp_core::model::primitives::Timestamp;
use crisp_core::verify::{ComplianceReport, verify};
use tracing::{debug, warn};

/// What crisp-core's offline pipeline found in one file.
#[derive(Debug)]
pub(super) struct Verdict {
    /// The report's `compliant`: phases A–D only (spec §11.2).
    compliant: bool,
    /// Every failed check as `(phase, "ID CODE: detail")`, in report order.
    failing: Vec<(char, String)>,
}

impl Verdict {
    /// Whether no check failed. Offline, a report's `compliant` covers only
    /// phases A–D, so a broken author signature (E1) still reports
    /// compliant; a passing file must also carry a valid author signature.
    /// E2 and E3 are skipped without platform keys.
    pub(super) fn passes(&self) -> bool {
        self.compliant && self.failing.is_empty()
    }

    /// The failures grouped by phase: `phase E: E1 E.SIG.AUTHOR: ...`.
    pub(super) fn reasons(&self) -> String {
        let mut phases: Vec<String> = Vec::new();
        let mut last = None;
        for (phase, line) in &self.failing {
            if last == Some(*phase) {
                if let Some(group) = phases.last_mut() {
                    group.push_str("; ");
                    group.push_str(line);
                }
            } else {
                phases.push(format!("phase {phase}: {line}"));
                last = Some(*phase);
            }
        }
        if phases.is_empty() {
            "not compliant, with no failing check reported".to_owned()
        } else {
            phases.join(" | ")
        }
    }
}

/// Runs crisp-core's offline A–E pipeline over `bytes`, described in logs
/// and errors as `subject`, off the async worker. Returns the bytes with
/// the verdict.
///
/// # Errors
///
/// Returns a sentence naming `subject` when the verification task does not
/// finish.
pub(super) async fn verdict(
    subject: &str,
    bytes: Vec<u8>,
    now: Timestamp,
) -> Result<(Vec<u8>, Verdict), String> {
    let (bytes, report) = tokio::task::spawn_blocking(move || {
        let report = verify(&bytes, &[], now);
        (bytes, report)
    })
    .await
    .map_err(|e| format!("verifying {subject} did not finish: {e}"))?;
    let verdict = Verdict {
        compliant: report.compliant,
        failing: failures(&report),
    };
    debug!(
        subject,
        compliant = verdict.compliant,
        trust = ?report.trust,
        passes = verdict.passes(),
        failing = ?verdict.failing,
        "file verified"
    );
    Ok((bytes, verdict))
}

/// Verifies the `source` (`"library"` or `"server"`) copy of `doc_id` as
/// the base of an edit. Returns the bytes when no check fails.
///
/// # Errors
///
/// Returns a sentence naming the document, every failing phase and check,
/// and that the copy was not used.
pub(super) async fn verified_base(
    doc_id: &str,
    source: &'static str,
    bytes: Vec<u8>,
    now: Timestamp,
) -> Result<Vec<u8>, String> {
    let subject = format!("the {source} copy of {doc_id}");
    let (bytes, verdict) = verdict(&subject, bytes, now).await?;
    if verdict.passes() {
        return Ok(bytes);
    }
    warn!(doc_id, source, failing = ?verdict.failing, "edit base refused");
    let what = if source == "library" {
        "library file"
    } else {
        "server copy"
    };
    Err(format!(
        "{subject} does not verify ({}); the {what} was not used as the base of this edit",
        verdict.reasons()
    ))
}

/// Every failed check as `(phase, "ID CODE: detail")`, in report order. A
/// check id's letter is its phase (spec §11.1).
fn failures(report: &ComplianceReport) -> Vec<(char, String)> {
    report
        .failures()
        .into_iter()
        .map(|check| {
            let phase = check.id.chars().next().unwrap_or('?');
            let mut line = check.id.clone();
            if let Some(code) = &check.code {
                line = format!("{line} {code}");
            }
            if let Some(detail) = &check.detail {
                line = format!("{line}: {detail}");
            }
            (phase, line)
        })
        .collect()
}
