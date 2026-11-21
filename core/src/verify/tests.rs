//! Verification tests, driven by the compliance suite (spec §11.5).
//!
//! Every case below is a real, deterministically signed bundle crafted to
//! fail exactly one check with one code (or to pass everything). The same
//! table is written to `spec/compliance/` — run with
//! `CRISP_UPDATE_COMPLIANCE=1` to regenerate it; otherwise the committed
//! suite must match byte for byte.

use std::collections::BTreeMap;

use ciborium::value::Value;

use super::*;
use crate::author::{Signing, author_sign, endorse_with, prepare};
use crate::codec::{encode_file, to_canonical_cbor};
use crate::fixtures::{draft_payload, rich_draft};
use crate::model::blob::{Blob, MediaType};
use crate::model::envelope::{CrispEnvelope, SigProtected};
use crate::model::ids::RecipeId;
use crate::model::primitives::{LanguageTag, OpenCode, Quantity};
use crate::model::recipe::{StepIngredient, Substitute};
use crate::sign::{SecretKeySet, signing_message};

mod suite;
mod trust;

/// When every suite bundle is verified.
const VERIFIED_AT: Timestamp = Timestamp(1_800_000_000);
/// When every suite bundle is authored.
const AUTHORED_AT: Timestamp = Timestamp(1_760_000_000);
/// When every suite bundle is endorsed.
const ENDORSED_AT: Timestamp = Timestamp(1_760_000_060);

fn author() -> SecretKeySet {
    SecretKeySet::from_seed(&[0xA1; 32])
}

fn platform() -> SecretKeySet {
    SecretKeySet::from_seed(&[0xB1; 32])
}

fn retired_platform() -> SecretKeySet {
    SecretKeySet::from_seed(&[0xB2; 32])
}

/// The trusted key set every suite bundle is verified against.
fn platform_keys() -> Vec<PlatformKey> {
    vec![
        PlatformKey {
            keyset: platform().public_keyset(),
            valid_from: Timestamp(1_700_000_000),
            valid_until: None,
        },
        PlatformKey {
            keyset: retired_platform().public_keyset(),
            valid_from: Timestamp(1_700_000_000),
            valid_until: Timestamp(1_750_000_000).into(),
        },
    ]
}

/// Prepares a draft for the suite author with a fixed nonce.
fn prepared(draft: &BundlePayload) -> BundlePayload {
    let mut nonce = || Ok(vec![0x5A; 16]);
    prepare(
        draft,
        &author(),
        "Suite Author",
        AUTHORED_AT,
        None,
        &mut nonce,
    )
    .expect("prepare")
}

/// Author-signs and endorses `payload` deterministically.
fn published(payload: &BundlePayload) -> CrispEnvelope {
    let signed =
        author_sign(payload, &author(), AUTHORED_AT, Signing::Deterministic).expect("sign");
    endorse_with(&signed, &platform(), ENDORSED_AT, Signing::Deterministic).expect("endorse")
}

fn file(envelope: &CrispEnvelope) -> Vec<u8> {
    encode_file(envelope).expect("encode")
}

/// Re-derives the revision and doc id after a mutation, so only the
/// intended check fails.
fn rederive(payload: &mut BundlePayload) {
    for recipe in &mut payload.recipes {
        recipe.revision = crate::identity::derive_revision(recipe).expect("revision");
    }
    if let Some(first) = payload.recipes.first() {
        payload.doc_id = crate::model::ids::DocId(first.id.0.clone());
    }
}

/// Publishes a mutated copy of the minimal recipe.
fn mutated(mutate: impl FnOnce(&mut BundlePayload)) -> Vec<u8> {
    let mut payload = prepared(&draft_payload());
    mutate(&mut payload);
    rederive(&mut payload);
    file(&published(&payload))
}

/// Encodes a payload map with one raw edit applied after serialization, for
/// shapes the typed model cannot express.
fn raw_payload_edit(edit: impl FnOnce(&mut Vec<(Value, Value)>)) -> Vec<u8> {
    let payload = prepared(&draft_payload());
    let mut envelope = published(&payload);
    let Value::Map(mut entries) = Value::serialized(&payload).expect("value") else {
        unreachable!("a payload is a map")
    };
    edit(&mut entries);
    envelope.signed_bundle.payload =
        crate::cbor::encode_canonical(&Value::Map(entries)).expect("encode");
    file(&envelope)
}

/// Rewrites one signature's protected header and re-signs it.
fn resign(
    envelope: &mut CrispEnvelope,
    index: usize,
    key: &SecretKeySet,
    edit: impl FnOnce(&mut SigProtected),
) {
    let sig = &mut envelope.signed_bundle.signatures[index];
    let mut sp: SigProtected = crate::codec::from_canonical_cbor(&sig.protected).expect("sp");
    edit(&mut sp);
    sig.protected = to_canonical_cbor(&sp).expect("encode");
    let aad = sp.binds.clone().unwrap_or_default();
    let message =
        signing_message(&envelope.signed_bundle.protected, &sig.protected, &aad).expect("M");
    sig.sig_value = key.sign_deterministic(&message).expect("sign");
}

/// One compliance-suite case.
struct Case {
    /// The file name, without extension.
    name: &'static str,
    /// `None` for a valid bundle; else the one failing check and its code.
    expect: Option<(&'static str, ErrorCode)>,
    /// Builds the bundle bytes.
    build: fn() -> Vec<u8>,
}

mod cases;

use cases::cases;

fn png_blob(bytes: &[u8]) -> Blob {
    Blob {
        media_type: OpenCode::Known(MediaType::Png),
        size: bytes.len() as u64,
        width: None,
        height: None,
        bytes: bytes.to_vec(),
    }
}

/// Asserts a report matches a case's expectation exactly.
fn assert_case(name: &str, expect: Option<(&str, ErrorCode)>, report: &ComplianceReport) {
    let failures: Vec<(&str, Option<&str>)> = report
        .checks
        .iter()
        .filter(|c| c.status == CheckStatus::Fail)
        .map(|c| (c.id.as_str(), c.code.as_deref()))
        .collect();
    assert_eq!(
        report.checks.len(),
        CHECK_IDS.len(),
        "{name}: every check is reported"
    );
    match expect {
        None => {
            assert!(
                failures.is_empty(),
                "{name}: unexpected failures {failures:?}"
            );
            assert_eq!(report.class, ComplianceClass::Published, "{name}");
            assert_eq!(report.trust, Trust::Endorsed, "{name}");
        }
        Some((id, code)) => {
            assert_eq!(
                failures,
                vec![(id, Some(code.as_str()))],
                "{name}: {report:#?}"
            );
            assert_ne!(report.class, ComplianceClass::Published, "{name}");
        }
    }
}

#[test]
fn every_compliance_case_fails_exactly_its_one_check() {
    let (keys, revoked) = (platform_keys(), trust::revocations());
    for case in cases() {
        let report = verify_with_revocations(&(case.build)(), &keys, &revoked, VERIFIED_AT);
        assert_case(case.name, case.expect, &report);
    }
}

#[test]
fn every_error_code_has_a_compliance_case() {
    let covered: std::collections::BTreeSet<&str> = cases()
        .iter()
        .filter_map(|c| c.expect.map(|(_, code)| code.as_str()))
        .collect();
    for code in ErrorCode::ALL {
        assert!(
            covered.contains(code.as_str()),
            "no compliance case for {code}"
        );
    }
}

#[test]
fn a_fatal_failure_skips_every_later_check() {
    let report = verify(b"not a crisp file", &[], VERIFIED_AT);
    assert!(!report.compliant);
    assert_eq!(report.checks.len(), CHECK_IDS.len());
    assert_eq!(report.failures().len(), 1);
}

#[test]
fn a_co_author_cannot_sign_for_the_owner() {
    // The co-author is listed (so C5 passes), but the owner never signs.
    let co_author = SecretKeySet::from_seed(&[0xD1; 32]);
    let mut payload = prepared(&draft_payload());
    payload.meta.authors.push(crate::model::meta::Author {
        uid: co_author.user_id().expect("uid"),
        display_name: "Co".to_owned(),
        keyset: co_author.public_keyset(),
    });
    let signed =
        author_sign(&payload, &co_author, AUTHORED_AT, Signing::Deterministic).expect("sign");
    let report = verify(&file(&signed), &[], VERIFIED_AT);
    let e1 = report.checks.iter().find(|c| c.id == "E1").expect("E1");
    assert_eq!(e1.code.as_deref(), Some("E.SIG.MISSING"));
}

#[test]
fn issues_carry_json_pointer_paths() {
    let mut payload = prepared(&draft_payload());
    payload.recipes[0].steps[0].ingredients = Some(vec![StepIngredient {
        r#ref: 7,
        quantity: None,
    }]);
    let issues = integrity::references(&payload).0;
    assert_eq!(issues[0].path, "/recipes/0/steps/0/ingredients/0/ref");
}
