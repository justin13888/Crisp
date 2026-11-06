//! What phase E trusts beyond the bundle: the platform key set (absent when
//! offline) and the platform's revocation list (spec §8.8).

use super::*;

/// The suite bundle whose endorsement the suite's revocation list revokes:
/// the minimal recipe with a later `meta.modified_at`, so its content hash
/// differs from every valid case's.
fn revoked_bundle() -> Vec<u8> {
    mutated(|p| p.meta.modified_at = Timestamp(AUTHORED_AT.0 + 1))
}

/// The revocation list every suite bundle is verified against.
pub(super) fn revocations() -> Vec<Revocation> {
    let envelope = crate::codec::decode_file(&revoked_bundle()).expect("decode");
    vec![Revocation {
        content_hash: crate::crypto::hash::sha256(&envelope.signed_bundle.payload).to_vec(),
        revoked_at: Timestamp(ENDORSED_AT.0 + 3600),
    }]
}

/// The compliance case for `E.SIG.REVOKED`.
pub(super) fn case() -> Case {
    Case {
        name: "revoked-endorsement",
        expect: Some(("E2", ErrorCode::SigRevoked)),
        build: revoked_bundle,
    }
}

#[test]
fn offline_verification_stops_at_author_signed() {
    let bytes = file(&published(&prepared(&draft_payload())));
    let report = verify(&bytes, &[], VERIFIED_AT);
    assert!(report.compliant);
    assert_eq!(report.trust, Trust::AuthorSigned);
    assert_eq!(report.class, ComplianceClass::Structural);
    let skipped: Vec<&str> = report
        .checks
        .iter()
        .filter(|c| c.status == CheckStatus::Skipped)
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(skipped, ["E2", "E3"]);
}

#[test]
fn a_revoked_endorsement_counts_only_where_the_list_is_held() {
    let bytes = revoked_bundle();
    let report = verify(&bytes, &platform_keys(), VERIFIED_AT);
    assert_case("unrevoked", None, &report);
    let report = verify_with_revocations(&bytes, &[], &revocations(), VERIFIED_AT);
    assert_eq!(report.trust, Trust::AuthorSigned, "offline skips E2");
    let report = verify_with_revocations(&bytes, &platform_keys(), &revocations(), VERIFIED_AT);
    assert_eq!(report.trust, Trust::AuthorSigned);
    assert!(
        report.compliant,
        "a revocation withdraws trust, not structure"
    );
    let e3 = report.checks.iter().find(|c| c.id == "E3").expect("E3");
    assert_eq!(e3.status, CheckStatus::Skipped);
    let e2 = report.checks.iter().find(|c| c.id == "E2").expect("E2");
    assert!(
        e2.detail
            .as_deref()
            .is_some_and(|d| d.contains("1760003660"))
    );
}

#[test]
fn a_forged_endorsement_of_a_revoked_bundle_reports_the_forgery() {
    let mut envelope = crate::codec::decode_file(&revoked_bundle()).expect("decode");
    envelope.signed_bundle.signatures[1].sig_value[100] ^= 1;
    let report = verify_with_revocations(
        &file(&envelope),
        &platform_keys(),
        &revocations(),
        VERIFIED_AT,
    );
    assert_case("forged", Some(("E2", ErrorCode::SigEndorsement)), &report);
}
