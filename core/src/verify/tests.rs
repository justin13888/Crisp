//! Verification pipeline tests, built around real signed bundles.

use std::collections::BTreeMap;

use super::*;
use crate::FORMAT_VERSION;
use crate::build::{author_sign, endorse, finalize_recipe};
use crate::codec::encode_file;
use crate::fixtures::{english, sample_recipe};
use crate::identity::key_fingerprint;
use crate::model::ids::DocId;
use crate::model::meta::{Author, DocumentMeta};
use crate::model::payload::{BundlePayload, Kind};
use crate::model::primitives::{LanguageTag, Timestamp};
use crate::model::recipe::Recipe;
use crate::sign::derive_keyset;

/// Assembles a payload around a (possibly already finalized) recipe.
fn payload_for(recipe: Recipe, author: &crate::sign::SecretKeySet) -> BundlePayload {
    BundlePayload {
        v: FORMAT_VERSION,
        min_v: FORMAT_VERSION,
        kind: Kind::Recipe,
        doc_id: DocId(recipe.id.0.clone()),
        default_language: LanguageTag("en".to_owned()),
        languages: vec![LanguageTag("en".to_owned())],
        meta: DocumentMeta {
            title: english("Cookies"),
            authors: vec![Author {
                uid: author.user_id().expect("uid"),
                display_name: "Baker".to_owned(),
                keyset: author.public_keyset(),
            }],
            license: "CC0-1.0".to_owned(),
            created_at: Timestamp(1_000),
            modified_at: Timestamp(2_000),
            identity: None,
            description: None,
            cover: None,
        },
        recipes: vec![recipe],
        book: None,
        blobs: BTreeMap::new(),
        ext: None,
    }
}

/// Builds a fully author-signed and endorsed bundle.
fn published_bundle() -> (Vec<u8>, Vec<crate::model::meta::PublicKeySet>) {
    let author = derive_keyset(&[20u8; 32]);
    let platform = derive_keyset(&[21u8; 32]);
    let fpr = key_fingerprint(&author.public_keyset()).expect("fpr");
    let recipe = finalize_recipe(sample_recipe(), fpr).expect("finalize");
    let payload = payload_for(recipe, &author);
    let envelope = author_sign(&payload, &author, Timestamp(2_000)).expect("sign");
    let endorsed = endorse(&envelope, &platform, Timestamp(2_500)).expect("endorse");
    let bytes = encode_file(&endorsed).expect("encode");
    (bytes, vec![platform.public_keyset()])
}

#[test]
fn a_published_bundle_is_fully_compliant() {
    let (bytes, platform_keys) = published_bundle();
    let report = verify(&bytes, &platform_keys, Timestamp(3_000));
    assert!(
        report.failures().is_empty(),
        "unexpected failures: {:?}",
        report.failures()
    );
    assert!(report.compliant);
    assert_eq!(report.class, ComplianceClass::Published);
    assert_eq!(report.trust, Trust::Endorsed);
    assert_eq!(report.format_version, FORMAT_VERSION);
}

#[test]
fn without_platform_keys_a_bundle_is_structurally_compliant() {
    let (bytes, _) = published_bundle();
    let report = verify(&bytes, &[], Timestamp(3_000));
    assert!(report.compliant);
    assert_eq!(report.class, ComplianceClass::Structural);
    assert_eq!(report.trust, Trust::AuthorSigned);
    // E2 is skipped when no platform keys are supplied.
    let e2 = report.checks.iter().find(|c| c.id == "E2").expect("E2");
    assert_eq!(e2.status, CheckStatus::Skipped);
}

#[test]
fn garbage_input_is_rejected_in_phase_a() {
    let report = verify(b"not a crisp file", &[], Timestamp(3_000));
    assert!(!report.compliant);
    assert!(report.failures().iter().any(|c| c.id.starts_with('A')));
}

#[test]
fn a_tampered_byte_breaks_verification() {
    let (mut bytes, platform_keys) = published_bundle();
    // Flip a byte deep inside the file. Wherever it lands — payload, header,
    // or a signature — at least one check must fail and the bundle can no
    // longer be Published-compliant.
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    let report = verify(&bytes, &platform_keys, Timestamp(3_000));
    assert!(
        !report.failures().is_empty(),
        "a tampered bundle must fail at least one check"
    );
    assert!(
        !(report.compliant && report.class == ComplianceClass::Published),
        "a tampered bundle must not be Published-compliant"
    );
}

#[test]
fn a_stale_recipe_id_fails_check_c3() {
    let author = derive_keyset(&[22u8; 32]);
    let platform = derive_keyset(&[23u8; 32]);
    let fpr = key_fingerprint(&author.public_keyset()).expect("fpr");
    let mut recipe = finalize_recipe(sample_recipe(), fpr).expect("finalize");
    // Change the identity after the id was derived: the id no longer matches.
    recipe.identity.title = "A Different Title".to_owned();
    let payload = payload_for(recipe, &author);
    let envelope = author_sign(&payload, &author, Timestamp(2_000)).expect("sign");
    let endorsed = endorse(&envelope, &platform, Timestamp(2_500)).expect("endorse");
    let bytes = encode_file(&endorsed).expect("encode");

    let report = verify(&bytes, &[platform.public_keyset()], Timestamp(3_000));
    assert!(!report.compliant);
    let c3 = report.checks.iter().find(|c| c.id == "C3").expect("C3");
    assert_eq!(c3.status, CheckStatus::Fail);
    assert_eq!(c3.code.as_deref(), Some("E.ID.MISMATCH"));
}
