//! The A–E verification an edit base must pass, wherever it came from.

use crisp_core::model::primitives::Timestamp;
use crisp_core::verify::{ComplianceReport, verify};
use tracing::{debug, warn};

/// Runs crisp-core's offline A–E pipeline over `bytes`, the `source`
/// (`"library"` or `"server"`) copy of `doc_id`, off the async worker.
/// Returns the bytes when no check fails.
///
/// Offline, a report's `compliant` covers only phases A–D (the Structural
/// class, spec §11.2), so a broken author signature (E1) still reports
/// compliant. A base must also carry a valid author signature, so any
/// failed check refuses it; E2 and E3 are skipped without platform keys.
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
    let (bytes, report) = tokio::task::spawn_blocking(move || {
        let report = verify(&bytes, &[], now);
        (bytes, report)
    })
    .await
    .map_err(|e| format!("verifying the {source} copy of {doc_id} did not finish: {e}"))?;
    let failing = failures(&report);
    let accepted = report.compliant && failing.is_empty();
    debug!(
        doc_id,
        source,
        compliant = report.compliant,
        trust = ?report.trust,
        accepted,
        failing = ?failing,
        "edit base verified"
    );
    if accepted {
        return Ok(bytes);
    }
    warn!(doc_id, source, failing = ?failing, "edit base refused");
    Err(refusal(doc_id, source, &failing))
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

/// The refusal: failures grouped by phase.
fn refusal(doc_id: &str, source: &str, failing: &[(char, String)]) -> String {
    let mut phases: Vec<String> = Vec::new();
    let mut last = None;
    for (phase, line) in failing {
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
    let what = if source == "library" {
        "library file"
    } else {
        "server copy"
    };
    let reasons = if phases.is_empty() {
        "not compliant, with no failing check reported".to_owned()
    } else {
        phases.join(" | ")
    };
    format!(
        "the {source} copy of {doc_id} does not verify ({reasons}); the {what} was not used as \
         the base of this edit"
    )
}
