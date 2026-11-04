//! The published compliance suite (spec §11.5): writes, or checks, every
//! case of [`super::cases`] under `spec/compliance/` with its `vectors.json`.

use std::path::PathBuf;

use super::*;

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
