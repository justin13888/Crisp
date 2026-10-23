//! The staged verification pipeline (spec §11).
//!
//! [`verify`] runs the deterministic A–E procedure over a `.crisp` file and
//! emits a [`ComplianceReport`]. Phases A and B are fatal — the first failure
//! stops verification and every later check is reported `skipped`. Phases
//! C, D, and E are collect-all. Every field is attacker-controlled until a
//! check clears it.

pub(crate) mod bounds;
pub mod integrity;
pub mod issue;
pub mod policy;
pub mod report;
pub(crate) mod schema;
pub mod signatures;
pub(crate) mod walk;

pub use issue::Issue;
pub use report::{CheckResult, CheckStatus, ComplianceClass, ComplianceReport, Trust};
pub use signatures::{ENDORSEMENT_SKEW_SECS, PlatformKey, author_binding};

use tracing::{debug, info_span};

use crate::FORMAT_VERSION;
use crate::cbor::{decode_canonical, scan_canonical, scan_canonical_file};
use crate::codec::{decode_file, from_canonical_cbor};
use crate::error::{CrispError, ErrorCode, Result};
use crate::limits::MAX_BUNDLE_SIZE;
use crate::model::envelope::{CONTAINER_VERSION, CoseSign, ProtectedHeader};
use crate::model::payload::{BundlePayload, Kind};
use crate::model::primitives::Timestamp;
use issue::Issues;
use report::CRISP_REPORT_MARKER;

/// Every check id, in report order.
pub const CHECK_IDS: [&str; 22] = [
    "A1", "A2", "A3", "A4", "A5", "B1", "B2", "B3", "B4", "C1", "C2", "C3", "C4", "C5", "C6", "D1",
    "D2", "D3", "D4", "E1", "E2", "E3",
];

/// Verifies a `.crisp` file and returns its compliance report.
///
/// `platform_keys` are the trusted endorsement keys; pass an empty slice for
/// offline verification (E2 and E3 are then skipped and trust tops out at
/// `author-signed`). `verified_at` is stamped into the report.
#[must_use]
pub fn verify(
    bytes: &[u8],
    platform_keys: &[PlatformKey],
    verified_at: Timestamp,
) -> ComplianceReport {
    let span = info_span!("verify", bytes = bytes.len());
    let _guard = span.enter();
    let mut checks = Checks::default();

    let Some(decoded) = fatal_phases(&mut checks, bytes) else {
        return checks.finish(0, Trust::Unsigned, verified_at);
    };
    let payload = &decoded.payload;
    for (id, issues) in
        structural_checks(payload, Some((&decoded.cose.payload, &decoded.protected)))
    {
        checks.issues(id, issues);
    }

    let parsed = signatures::parse(&decoded.cose);
    checks.run("E1", || {
        signatures::authors(&decoded.cose, payload, &parsed)
    });
    let author_ok = checks.passed("E1");
    if platform_keys.is_empty() {
        checks.skip("E2");
        checks.skip("E3");
    } else {
        checks.run("E2", || {
            signatures::endorsement(&decoded.cose, &parsed, platform_keys)
        });
        if checks.passed("E2") {
            checks.run("E3", || signatures::timing(payload, &parsed));
        } else {
            checks.skip("E3");
        }
    }
    let trust = if author_ok && checks.passed("E2") {
        Trust::Endorsed
    } else if author_ok {
        Trust::AuthorSigned
    } else {
        Trust::Unsigned
    };
    let report = checks.finish(payload.v, trust, verified_at);
    debug!(
        compliant = report.compliant,
        ?trust,
        failures = report.failures().len(),
        "verified"
    );
    report
}

/// Phases C and D (plus the C4 hash when the raw bytes are known), as
/// `(check id, issues)` pairs. Shared by [`verify`] and draft validation.
pub(crate) fn structural_checks(
    payload: &BundlePayload,
    raw: Option<(&[u8], &ProtectedHeader)>,
) -> Vec<(&'static str, Issues)> {
    let mut out = vec![
        ("C1", integrity::blobs(payload)),
        ("C2", integrity::ids(payload)),
        ("C3", integrity::derivations(payload)),
    ];
    if let Some((bytes, protected)) = raw {
        out.push((
            "C4",
            integrity::content_hash(bytes, &protected.content_hash),
        ));
    }
    out.extend([
        ("C5", integrity::author_binding(payload)),
        ("C6", integrity::references(payload)),
        ("D1", policy::links(payload)),
        ("D2", policy::localization(payload)),
        ("D3", policy::time_order(payload)),
        ("D4", policy::quantities(payload)),
    ]);
    out
}

/// Phase B checks on a decoded payload, as `(check id, issues)` pairs.
pub(crate) fn shape_checks(payload: &BundlePayload) -> Vec<(&'static str, Issues)> {
    vec![
        ("B1", schema::check_payload(payload)),
        ("B2", version_issues(payload)),
        ("B3", bounds::check(payload)),
        ("B4", consistency(payload, None)),
    ]
}

/// The artifacts produced once the fatal phases have passed.
struct Decoded {
    cose: CoseSign,
    protected: ProtectedHeader,
    payload: BundlePayload,
}

/// Runs phases A and B; returns `None` on the first fatal failure.
fn fatal_phases(checks: &mut Checks, bytes: &[u8]) -> Option<Decoded> {
    if bytes.len() > MAX_BUNDLE_SIZE {
        checks.fail(
            "A3",
            &CrispError::check(
                ErrorCode::Limit,
                format!(
                    "the file is {} bytes; the maximum is {MAX_BUNDLE_SIZE}",
                    bytes.len()
                ),
            ),
        );
        return None;
    }
    if let Err(e) = scan_canonical_file(bytes) {
        let id = if e.code() == Some(ErrorCode::CborMalformed) {
            "A3"
        } else {
            "A4"
        };
        checks.fail(id, &e);
        return None;
    }
    checks.pass("A3");
    checks.pass("A4");

    let envelope = match decode_file(bytes) {
        Ok(envelope) => envelope,
        Err(e) => {
            checks.fail(
                if e.code() == Some(ErrorCode::Schema) {
                    "B1"
                } else {
                    "A1"
                },
                &e,
            );
            return None;
        }
    };
    checks.pass("A1");

    if envelope.container_version != CONTAINER_VERSION {
        checks.fail(
            "A2",
            &CrispError::check(
                ErrorCode::Container,
                format!(
                    "unsupported container version {}",
                    envelope.container_version
                ),
            ),
        );
        return None;
    }
    checks.pass("A2");

    let cose = envelope.signed_bundle;
    let embedded = [&cose.protected, &cose.payload]
        .into_iter()
        .chain(cose.signatures.iter().map(|s| &s.protected));
    for blob in embedded {
        if let Err(e) = scan_canonical(blob) {
            checks.fail("A5", &e);
            return None;
        }
    }
    checks.pass("A5");

    let protected: ProtectedHeader = match from_canonical_cbor(&cose.protected) {
        Ok(value) => value,
        Err(e) => {
            checks.fail("B1", &e);
            return None;
        }
    };
    let payload = match decode_payload(&cose.payload) {
        Ok(payload) => payload,
        Err(issues) => {
            checks.issues("B1", issues);
            return None;
        }
    };
    let mut shape = shape_checks(&payload);
    if let Some((_, b4)) = shape.iter_mut().find(|(id, _)| *id == "B4") {
        *b4 = consistency(&payload, Some(&protected));
    }
    for (id, issues) in shape {
        let failed = !issues.0.is_empty();
        checks.issues(id, issues);
        if failed {
            return None;
        }
    }
    Some(Decoded {
        cose,
        protected,
        payload,
    })
}

/// Decodes payload bytes, applying the Value-level B1 rules first so their
/// failures carry precise paths.
fn decode_payload(bytes: &[u8]) -> core::result::Result<BundlePayload, Issues> {
    let value = decode_canonical(bytes).map_err(|e| {
        let mut issues = Issues::default();
        issues.0.push(Issue::from_error("", &e));
        issues
    })?;
    let issues = schema::check_value(&value);
    if !issues.0.is_empty() {
        return Err(issues);
    }
    value.deserialized().map_err(|e| {
        let mut issues = Issues::default();
        issues.push(
            "",
            ErrorCode::Schema,
            format!("the payload does not match the schema: {e}"),
        );
        issues
    })
}

/// B2 — the format version is within this reader's supported range.
fn version_issues(payload: &BundlePayload) -> Issues {
    let mut issues = Issues::default();
    issues.require(
        payload.min_v <= FORMAT_VERSION,
        "/min_v",
        ErrorCode::Version,
        || {
            format!(
                "the bundle needs reader version {:#08x}; this reader is {FORMAT_VERSION:#08x}",
                payload.min_v
            )
        },
    );
    issues.require(
        payload.v >> 16 == FORMAT_VERSION >> 16,
        "/v",
        ErrorCode::Version,
        || format!("unsupported major format version in {:#08x}", payload.v),
    );
    issues.require(
        payload.min_v <= payload.v,
        "/min_v",
        ErrorCode::Version,
        || "min_v must not exceed v".to_owned(),
    );
    issues
}

/// B4 — `kind`, `book`, `meta.identity`, languages, and (when given) the
/// protected header agree.
fn consistency(payload: &BundlePayload, protected: Option<&ProtectedHeader>) -> Issues {
    let mut issues = Issues::default();
    let c = ErrorCode::Consistency;
    issues.require(
        payload.languages.contains(&payload.default_language),
        "/default_language",
        c,
        || "default_language is not listed in languages".to_owned(),
    );
    let mut seen = std::collections::BTreeSet::new();
    for (i, tag) in payload.languages.iter().enumerate() {
        issues.require(seen.insert(tag), &format!("/languages/{i}"), c, || {
            format!("language {:?} is listed twice", tag.0)
        });
    }
    match payload.kind {
        Kind::Recipe => {
            issues.require(payload.book.is_none(), "/book", c, || {
                "a recipe bundle must not carry a book structure".to_owned()
            });
            issues.require(payload.meta.identity.is_none(), "/meta/identity", c, || {
                "a recipe bundle must not carry meta.identity".to_owned()
            });
            issues.require(payload.recipes.len() == 1, "/recipes", c, || {
                "a recipe bundle holds exactly one recipe".to_owned()
            });
        }
        Kind::Book => {
            issues.require(payload.book.is_some(), "/book", c, || {
                "a book bundle must carry a book structure".to_owned()
            });
            issues.require(payload.meta.identity.is_some(), "/meta/identity", c, || {
                "a book bundle must carry meta.identity".to_owned()
            });
        }
    }
    if let Some(protected) = protected {
        issues.require(protected.format_version == payload.v, "/v", c, || {
            "the protected header's format version differs from the payload's".to_owned()
        });
        issues.require(protected.doc_id == payload.doc_id, "/doc_id", c, || {
            "the protected header's doc_id differs from the payload's".to_owned()
        });
    }
    issues
}

/// Accumulates check results during verification.
#[derive(Default)]
struct Checks {
    results: Vec<CheckResult>,
}

impl Checks {
    fn record(&mut self, id: &str, status: CheckStatus, error: Option<&CrispError>) {
        self.results.push(CheckResult {
            id: id.to_owned(),
            status,
            code: error
                .and_then(CrispError::code)
                .map(|c| c.as_str().to_owned()),
            detail: error.map(|e| match e {
                CrispError::Check { detail, .. } => detail.clone(),
                other => other.to_string(),
            }),
        });
    }

    fn pass(&mut self, id: &str) {
        self.record(id, CheckStatus::Pass, None);
    }

    fn skip(&mut self, id: &str) {
        self.record(id, CheckStatus::Skipped, None);
    }

    fn fail(&mut self, id: &str, error: &CrispError) {
        debug!(check = id, %error, "check failed");
        self.record(id, CheckStatus::Fail, Some(error));
    }

    /// Records a collect-all check from its issues: pass when empty, else a
    /// failure naming the first issue and how many more there are.
    fn issues(&mut self, id: &str, issues: Issues) {
        let Some(first) = issues.0.first() else {
            self.pass(id);
            return;
        };
        let mut error = first.to_error();
        if issues.0.len() > 1
            && let CrispError::Check { detail, .. } = &mut error
        {
            detail.push_str(&format!(" (+{} more)", issues.0.len() - 1));
        }
        self.fail(id, &error);
    }

    fn run(&mut self, id: &str, check: impl FnOnce() -> Result<()>) {
        match check() {
            Ok(()) => self.pass(id),
            Err(e) => self.fail(id, &e),
        }
    }

    fn passed(&self, id: &str) -> bool {
        self.results
            .iter()
            .any(|c| c.id == id && c.status == CheckStatus::Pass)
    }

    fn phase_failed(&self, phases: &[char]) -> bool {
        self.results.iter().any(|c| {
            c.status == CheckStatus::Fail
                && c.id.chars().next().is_some_and(|p| phases.contains(&p))
        })
    }

    /// Builds the report: every check id appears once, missing ones skipped.
    fn finish(
        mut self,
        format_version: u64,
        trust: Trust,
        verified_at: Timestamp,
    ) -> ComplianceReport {
        for id in CHECK_IDS {
            if !self.results.iter().any(|c| c.id == id) {
                self.skip(id);
            }
        }
        self.results.sort_by(|a, b| a.id.cmp(&b.id));
        let structural_ok = !self.phase_failed(&['A', 'B', 'C', 'D']);
        let signatures_ok = !self.phase_failed(&['E']);
        let class = if structural_ok && signatures_ok && trust == Trust::Endorsed {
            ComplianceClass::Published
        } else {
            ComplianceClass::Structural
        };
        ComplianceReport {
            crisp_report: CRISP_REPORT_MARKER,
            compliant: structural_ok,
            class,
            format_version,
            trust,
            verified_at,
            checks: self.results,
        }
    }
}

#[cfg(test)]
mod tests;

/// Totality: [`verify`] returns a whole report for any input, never panics.
#[cfg(test)]
mod totality {
    use std::path::{Path, PathBuf};

    use proptest::prelude::*;

    use super::*;

    const AT: Timestamp = Timestamp(1_800_000_000);

    /// The suite, read once for every case.
    static FIXTURES: std::sync::LazyLock<Vec<Vec<u8>>> = std::sync::LazyLock::new(fixtures);

    /// Every committed compliance-suite bundle, valid and invalid.
    fn fixtures() -> Vec<Vec<u8>> {
        fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("read the suite") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    collect(&path, out);
                } else if path.extension().is_some_and(|e| e == "crisp") {
                    out.push(path);
                }
            }
        }
        let mut paths = Vec::new();
        collect(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../spec/compliance"),
            &mut paths,
        );
        paths.sort();
        assert!(!paths.is_empty(), "the compliance suite is missing");
        paths
            .iter()
            .map(|p| std::fs::read(p).expect("read fixture"))
            .collect()
    }

    /// Asserts `report` lists every check exactly once.
    fn assert_whole(report: &ComplianceReport) -> core::result::Result<(), TestCaseError> {
        let ids: Vec<&str> = report.checks.iter().map(|c| c.id.as_str()).collect();
        prop_assert_eq!(ids, CHECK_IDS.to_vec());
        Ok(())
    }

    proptest! {
        #[test]
        fn arbitrary_bytes_yield_a_whole_report(
            bytes in prop::collection::vec(any::<u8>(), 0..512),
        ) {
            let report = verify(&bytes, &[], AT);
            assert_whole(&report)?;
            prop_assert!(!report.compliant, "random bytes are never a signed bundle");
        }

        #[test]
        fn bit_flipped_fixtures_yield_a_whole_report(
            fixture in any::<prop::sample::Index>(),
            byte in any::<prop::sample::Index>(),
            bit in 0u8..8,
        ) {
            let mut bytes = fixture.get(&FIXTURES).clone();
            let at = byte.index(bytes.len());
            bytes[at] ^= 1 << bit;
            assert_whole(&verify(&bytes, &[], AT))?;
        }
    }
}
