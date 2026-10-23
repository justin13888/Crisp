//! The OpenAPI contract test.
//!
//! `spec/openapi.json` is the committed OpenAPI 3.2 description of the API,
//! emitted from the same router the server runs. This test fails if the code
//! and the committed document drift apart.
//!
//! Regenerate the document after an intentional API change with:
//!
//! ```bash
//! UPDATE_OPENAPI=1 cargo test -p crisp-server --test openapi
//! ```

use crisp_server::config::HttpOptions;
use kynos::openapi::SpecVersion;

/// Where the committed document lives.
const SPEC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../spec/openapi.json");

/// Emits the document for the default HTTP options, as pretty JSON with a
/// trailing newline.
fn emitted() -> String {
    let router = crisp_server::router(&HttpOptions::default());
    let document = router
        .openapi_as(SpecVersion::V3_2)
        .expect("the router describes itself as OpenAPI 3.2");
    let json = document.to_json().expect("the document serializes");
    // Re-pretty-print through serde_json so formatting is stable and diffable.
    let value: serde_json::Value = serde_json::from_str(&json).expect("emitted JSON parses");
    let mut pretty = serde_json::to_string_pretty(&value).expect("pretty JSON");
    pretty.push('\n');
    pretty
}

#[test]
fn router_validates_without_warnings() {
    let violations = crisp_server::router(&HttpOptions::default())
        .validate()
        .expect("the router can be described");
    assert!(
        violations.is_empty(),
        "the router has violations:\n{}",
        violations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn committed_openapi_document_matches_the_router() {
    let fresh = emitted();
    if std::env::var_os("UPDATE_OPENAPI").is_some_and(|v| v == "1") {
        std::fs::write(SPEC_PATH, &fresh).expect("write spec/openapi.json");
        eprintln!("wrote {SPEC_PATH}");
        return;
    }
    let committed = std::fs::read_to_string(SPEC_PATH).unwrap_or_default();
    assert!(
        committed == fresh,
        "spec/openapi.json is out of date with the router; regenerate it with \
         `UPDATE_OPENAPI=1 cargo test -p crisp-server --test openapi`"
    );
}

#[test]
fn bundle_bodies_are_octet_streams() {
    let document: serde_json::Value = serde_json::from_str(&emitted()).unwrap();
    let publish = &document["paths"]["/api/v1/bundles"]["post"];
    assert!(
        publish["requestBody"]["content"]["application/octet-stream"].is_object(),
        "publish must accept application/octet-stream"
    );
    assert!(publish["responses"]["201"]["content"]["application/octet-stream"].is_object());
    assert!(
        publish["responses"]["413"].is_object(),
        "publish declares its size cap"
    );
    assert!(
        document["components"]["securitySchemes"]["sessionToken"].is_object(),
        "the bearer scheme has a stable name"
    );
}
