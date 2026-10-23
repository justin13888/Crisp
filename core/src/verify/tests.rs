//! Verification tests, driven by the compliance suite (spec §11.5).
//!
//! Every case below is a real, deterministically signed bundle crafted to
//! fail exactly one check with one code (or to pass everything). The same
//! table is written to `spec/compliance/` — run with
//! `CRISP_UPDATE_COMPLIANCE=1` to regenerate it; otherwise the committed
//! suite must match byte for byte.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ciborium::value::Value;

use super::*;
use crate::author::{Signing, author_sign, endorse_with, prepare};
use crate::codec::{encode_file, to_canonical_cbor};
use crate::fixtures::{draft_payload, rich_draft};
use crate::model::blob::{Blob, MediaType};
use crate::model::envelope::{CrispEnvelope, SigProtected};
use crate::model::ids::RecipeId;
use crate::model::primitives::{LanguageTag, OpenCode, Quantity};
use crate::model::recipe::StepIngredient;
use crate::sign::{SecretKeySet, signing_message};

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

fn cases() -> Vec<Case> {
    use ErrorCode as E;
    vec![
        Case {
            name: "minimal",
            expect: None,
            build: || file(&published(&prepared(&draft_payload()))),
        },
        Case {
            name: "full-recipe",
            expect: None,
            build: || file(&published(&prepared(&rich_draft()))),
        },
        Case {
            name: "missing-magic",
            expect: Some(("A1", E::Magic)),
            build: || {
                let envelope = published(&prepared(&draft_payload()));
                to_canonical_cbor(&envelope).expect("encode")
            },
        },
        Case {
            name: "container-version",
            expect: Some(("A2", E::Container)),
            build: || {
                let mut envelope = published(&prepared(&draft_payload()));
                envelope.container_version = 2;
                file(&envelope)
            },
        },
        Case {
            name: "truncated",
            expect: Some(("A3", E::CborMalformed)),
            build: || {
                let mut bytes = file(&published(&prepared(&draft_payload())));
                bytes.truncate(bytes.len() - 10);
                bytes
            },
        },
        Case {
            name: "non-shortest-integer",
            expect: Some(("A4", E::CborNonDeterministic)),
            build: || {
                let bytes = file(&published(&prepared(&draft_payload())));
                // Tags (3 + 5 bytes), the 2-array head, then container version 1.
                assert_eq!(&bytes[8..10], &[0x82, 0x01]);
                [&bytes[..9], &[0x18, 0x01], &bytes[10..]].concat()
            },
        },
        Case {
            name: "indefinite-array",
            expect: Some(("A4", E::CborIndefinite)),
            build: || {
                let bytes = file(&published(&prepared(&draft_payload())));
                [&bytes[..8], &[0x9f], &bytes[9..], &[0xff]].concat()
            },
        },
        Case {
            name: "duplicate-key",
            expect: Some(("A5", E::CborDupKey)),
            build: || {
                let mut envelope = published(&prepared(&draft_payload()));
                envelope.signed_bundle.protected = vec![0xa2, 0x01, 0x01, 0x01, 0x02];
                file(&envelope)
            },
        },
        Case {
            name: "embedded-tag",
            expect: Some(("A5", E::CborTag)),
            build: || {
                let mut envelope = published(&prepared(&draft_payload()));
                let mut tagged = vec![0xd9, 0xd9, 0xf7];
                tagged.extend_from_slice(&envelope.signed_bundle.protected);
                envelope.signed_bundle.protected = tagged;
                file(&envelope)
            },
        },
        Case {
            name: "unknown-field",
            expect: Some(("B1", E::Schema)),
            build: || {
                raw_payload_edit(|entries| {
                    entries.push((Value::Text("bogus".to_owned()), Value::Bool(true)));
                })
            },
        },
        Case {
            name: "null-field",
            expect: Some(("B1", E::Schema)),
            build: || {
                raw_payload_edit(|entries| {
                    entries.push((Value::Text("ext".to_owned()), Value::Null));
                })
            },
        },
        Case {
            name: "future-reader",
            expect: Some(("B2", E::Version)),
            build: || mutated(|p| p.min_v = 0x01_0100),
        },
        Case {
            name: "too-many-timers",
            expect: Some(("B3", E::Limit)),
            build: || {
                mutated(|p| {
                    let timer = crate::model::recipe::Timer {
                        duration: crate::fixtures::amount(1, "min"),
                        label: None,
                        passive: None,
                        ext: None,
                    };
                    p.recipes[0].steps[0].timers = Some(vec![timer; 9]);
                })
            },
        },
        Case {
            name: "undeclared-default-language",
            expect: Some(("B4", E::Consistency)),
            build: || mutated(|p| p.default_language = LanguageTag("fr".to_owned())),
        },
        Case {
            name: "blob-hash",
            expect: Some(("C1", E::BlobHash)),
            build: || {
                mutated(|p| {
                    let id = crate::identity::derive_blob_id(b"other").expect("id");
                    p.blobs.insert(id.clone(), png_blob(b"image"));
                    p.recipes[0].media = Some(vec![id]);
                })
            },
        },
        Case {
            name: "blob-size",
            expect: Some(("C1", E::BlobSize)),
            build: || {
                mutated(|p| {
                    let id = crate::identity::derive_blob_id(b"image").expect("id");
                    let mut blob = png_blob(b"image");
                    blob.size = 99;
                    p.blobs.insert(id.clone(), blob);
                    p.recipes[0].media = Some(vec![id]);
                })
            },
        },
        Case {
            name: "blob-media-type",
            expect: Some(("C1", E::BlobMediaType)),
            build: || {
                mutated(|p| {
                    let id = crate::identity::derive_blob_id(b"image").expect("id");
                    let mut blob = png_blob(b"image");
                    blob.media_type = OpenCode::Other("image/svg+xml".to_owned());
                    p.blobs.insert(id.clone(), blob);
                    p.recipes[0].media = Some(vec![id]);
                })
            },
        },
        Case {
            name: "id-checksum",
            expect: Some(("C2", E::IdChecksum)),
            build: || {
                mutated(|p| {
                    let mut forged = p.recipes[0].id.0.clone();
                    forged.replace_range(10..11, if &forged[10..11] == "q" { "p" } else { "q" });
                    p.recipes[0].derived_from = Some(RecipeId(forged));
                })
            },
        },
        Case {
            name: "id-mismatch",
            expect: Some(("C3", E::IdMismatch)),
            build: || {
                mutated(|p| {
                    let mut other = p.recipes[0].identity.clone();
                    other.birth_nonce = vec![1; 16];
                    p.recipes[0].id = crate::identity::derive_recipe_id(&other).expect("id");
                })
            },
        },
        Case {
            name: "rev-mismatch",
            expect: Some(("C3", E::RevMismatch)),
            build: || {
                let mut payload = prepared(&draft_payload());
                let other = crate::identity::derive_revision(&prepared(&rich_draft()).recipes[0])
                    .expect("rev");
                payload.recipes[0].revision = other;
                file(&published(&payload))
            },
        },
        Case {
            name: "content-hash",
            expect: Some(("C4", E::ContentHash)),
            build: || {
                let payload = prepared(&draft_payload());
                let mut envelope = published(&payload);
                let mut bytes = envelope.signed_bundle.payload.clone();
                // Same-length, still-canonical edit: the license "CC0-1.0" → "CC0-1.1".
                let at = bytes
                    .windows(7)
                    .position(|w| w == b"CC0-1.0")
                    .expect("license");
                bytes[at + 6] = b'1';
                envelope.signed_bundle.payload = bytes;
                file(&envelope)
            },
        },
        Case {
            name: "author-binding",
            expect: Some(("C5", E::AuthorBinding)),
            build: || mutated(|p| p.meta.authors[0].uid = platform().user_id().expect("uid")),
        },
        Case {
            name: "unresolved-ref",
            expect: Some(("C6", E::Ref)),
            build: || {
                mutated(|p| {
                    p.recipes[0].steps[0].ingredients = Some(vec![StepIngredient {
                        r#ref: 99,
                        quantity: None,
                    }]);
                })
            },
        },
        Case {
            name: "http-link",
            expect: Some(("D1", E::UrlScheme)),
            build: || {
                mutated(|p| {
                    p.recipes[0].source = Some(crate::model::recipe::Source {
                        url: Some("http://example.com".to_owned()),
                        ..Default::default()
                    });
                })
            },
        },
        Case {
            name: "missing-default-language",
            expect: Some(("D2", E::L10nDefault)),
            build: || {
                mutated(|p| {
                    let fr = LanguageTag("fr".to_owned());
                    p.languages.push(fr.clone());
                    p.recipes[0].ingredients[0].name = crate::model::text::LocalizedText(
                        BTreeMap::from([(fr, "Farine".to_owned())]),
                    );
                })
            },
        },
        Case {
            name: "undeclared-language",
            expect: Some(("D2", E::L10nUndeclared)),
            build: || {
                mutated(|p| {
                    p.recipes[0].ingredients[0]
                        .name
                        .0
                        .insert(LanguageTag("de".to_owned()), "Mehl".to_owned());
                })
            },
        },
        Case {
            name: "time-order",
            expect: Some(("D3", E::TimeOrder)),
            build: || mutated(|p| p.meta.created_at = Timestamp(AUTHORED_AT.0 + 1)),
        },
        Case {
            name: "empty-quantity",
            expect: Some(("D4", E::Quantity)),
            build: || mutated(|p| p.recipes[0].ingredients[0].quantity = Some(Quantity::default())),
        },
        Case {
            name: "owner-unsigned",
            expect: Some(("E1", E::SigMissing)),
            build: || {
                // A listed co-author signs; the recipe's own author never does.
                let co_author = SecretKeySet::from_seed(&[0xD1; 32]);
                let mut payload = prepared(&draft_payload());
                payload.meta.authors.push(crate::model::meta::Author {
                    uid: co_author.user_id().expect("uid"),
                    display_name: "Co-author".to_owned(),
                    keyset: co_author.public_keyset(),
                });
                let signed = author_sign(&payload, &co_author, AUTHORED_AT, Signing::Deterministic)
                    .expect("sign");
                file(
                    &endorse_with(&signed, &platform(), ENDORSED_AT, Signing::Deterministic)
                        .expect("endorse"),
                )
            },
        },
        Case {
            name: "bad-author-signature",
            expect: Some(("E1", E::SigAuthor)),
            build: || {
                let payload = prepared(&draft_payload());
                let mut signed =
                    author_sign(&payload, &author(), AUTHORED_AT, Signing::Deterministic)
                        .expect("sign");
                signed.signed_bundle.signatures[0].sig_value[100] ^= 1;
                file(
                    &endorse_with(&signed, &platform(), ENDORSED_AT, Signing::Deterministic)
                        .expect("endorse"),
                )
            },
        },
        Case {
            name: "unknown-platform-key",
            expect: Some(("E2", E::KeyUnknown)),
            build: || {
                let payload = prepared(&draft_payload());
                let signed = author_sign(&payload, &author(), AUTHORED_AT, Signing::Deterministic)
                    .expect("sign");
                let stranger = SecretKeySet::from_seed(&[0xC1; 32]);
                file(
                    &endorse_with(&signed, &stranger, ENDORSED_AT, Signing::Deterministic)
                        .expect("endorse"),
                )
            },
        },
        Case {
            name: "retired-platform-key",
            expect: Some(("E2", E::KeyWindow)),
            build: || {
                let payload = prepared(&draft_payload());
                let signed = author_sign(&payload, &author(), AUTHORED_AT, Signing::Deterministic)
                    .expect("sign");
                file(
                    &endorse_with(
                        &signed,
                        &retired_platform(),
                        ENDORSED_AT,
                        Signing::Deterministic,
                    )
                    .expect("endorse"),
                )
            },
        },
        Case {
            name: "bad-endorsement",
            expect: Some(("E2", E::SigEndorsement)),
            build: || {
                let mut envelope = published(&prepared(&draft_payload()));
                envelope.signed_bundle.signatures[1].sig_value[100] ^= 1;
                file(&envelope)
            },
        },
        Case {
            name: "unbound-endorsement",
            expect: Some(("E2", E::SigBinding)),
            build: || {
                let mut envelope = published(&prepared(&draft_payload()));
                resign(&mut envelope, 1, &platform(), |sp| {
                    sp.binds = Some(vec![0; 32])
                });
                file(&envelope)
            },
        },
        Case {
            name: "modified-after-endorsement",
            expect: Some(("E3", E::TimeOrder)),
            build: || {
                let payload = prepared(&draft_payload());
                let signed = author_sign(&payload, &author(), AUTHORED_AT, Signing::Deterministic)
                    .expect("sign");
                let early = Timestamp(AUTHORED_AT.0 - ENDORSEMENT_SKEW_SECS - 1);
                file(
                    &endorse_with(&signed, &platform(), early, Signing::Deterministic)
                        .expect("endorse"),
                )
            },
        },
    ]
}

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
    let keys = platform_keys();
    for case in cases() {
        let report = verify(&(case.build)(), &keys, VERIFIED_AT);
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

// ---- the published suite ----------------------------------------------------

/// One suite file: its path, bytes, and expected failure.
type SuiteFile = (String, Vec<u8>, Option<(&'static str, ErrorCode)>);

fn suite_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../spec/compliance")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The suite's `vectors.json`: platform keys, verification time, and each
/// bundle's hash and expected verdict.
fn vectors(files: &[SuiteFile]) -> String {
    let keys: Vec<serde_json::Value> = platform_keys()
        .iter()
        .map(|k| {
            serde_json::json!({
                "ed25519": hex(&k.keyset.ed25519),
                "ml-dsa-65": hex(&k.keyset.ml_dsa_65),
                "valid_from": k.valid_from.0,
                "valid_until": k.valid_until.map(|t| t.0),
            })
        })
        .collect();
    let cases: Vec<serde_json::Value> = files
        .iter()
        .map(|(path, bytes, expect)| {
            let report = verify(bytes, &platform_keys(), VERIFIED_AT);
            serde_json::json!({
                "file": path,
                "sha256": hex(&crate::crypto::hash::sha256(bytes)),
                "compliant": report.compliant,
                "class": report.class,
                "trust": report.trust,
                "failed_check": expect.map(|(id, _)| id),
                "code": expect.map(|(_, code)| code.as_str()),
            })
        })
        .collect();
    let doc = serde_json::json!({
        "crisp_compliance": 1,
        "format_version": FORMAT_VERSION,
        "verified_at": VERIFIED_AT.0,
        "platform_keys": keys,
        "cases": cases,
    });
    let mut text = serde_json::to_string_pretty(&doc).expect("json");
    text.push('\n');
    text
}

#[test]
fn the_published_compliance_suite_is_current() {
    let files: Vec<SuiteFile> = cases()
        .into_iter()
        .map(|case| {
            let path = match case.expect {
                None => format!("valid/{}.crisp", case.name),
                Some((_, code)) => format!("invalid/{}/{}.crisp", code.as_str(), case.name),
            };
            (path, (case.build)(), case.expect)
        })
        .collect();
    let root = suite_dir();
    let vectors_text = vectors(&files);
    if std::env::var_os("CRISP_UPDATE_COMPLIANCE").is_some() {
        let _ = std::fs::remove_dir_all(&root);
        for (path, bytes, _) in &files {
            let target = root.join(path);
            std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
            std::fs::write(target, bytes).expect("write");
        }
        std::fs::write(root.join("vectors.json"), &vectors_text).expect("write");
        return;
    }
    let committed = std::fs::read_to_string(root.join("vectors.json"))
        .expect("spec/compliance/vectors.json is missing; run with CRISP_UPDATE_COMPLIANCE=1");
    assert_eq!(
        committed, vectors_text,
        "the suite is stale; run with CRISP_UPDATE_COMPLIANCE=1"
    );
    for (path, bytes, _) in &files {
        let on_disk = std::fs::read(root.join(path)).expect("suite file");
        assert_eq!(&on_disk, bytes, "{path} is stale");
    }
}
