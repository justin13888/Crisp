//! End-to-end API tests.
//!
//! These drive the real `crisp-server` router — real handlers, real
//! `crisp-core` cryptography, real A–E verification — in-process through
//! kynos's `TestClient`, over an in-memory `MockStorage`. Every test also
//! checks that each response it observed conforms to the OpenAPI document,
//! and `every_declared_response_is_exercised` proves the document holds no
//! response the server cannot actually produce.

mod common;

use std::sync::Arc;

use common::{
    Fault, Harness, MockStorage, PLATFORM_SEED, RecipeSpec, assert_code, build_recipe_bundle,
    build_recipe_bundle_with, endorse_with, now, platform, signed_profile, test_config,
};
use crisp_core::model::primitives::Timestamp;
use crisp_core::sign::SecretKeySet;
use crisp_core::verify::{Trust, verify};
use crisp_server::config::HttpOptions;
use crisp_server::error::code;
use crisp_server::search::RecipePage;
use crisp_server::version::{Health, HealthStatus, ServerInfo};
use kynos::http::StatusCode;
use kynos::test::{Coverage, TestClient, TestResponse};
use serde_json::{Value, json};

/// Sends an authenticated `POST /auth/logout`.
async fn logout(harness: &Harness, token: &str) -> TestResponse {
    harness
        .client
        .post("/api/v1/auth/logout")
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await
}

// ---------------------------------------------------------------------------
// Versioning, health, and middleware
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_and_server_info_report_versions() {
    let h = Harness::new();

    let response = h.client.get("/api/v1/health").send().await;
    response.assert_status(StatusCode::OK);
    let health: Health = response.json();
    assert_eq!(health.status, HealthStatus::Ok);

    let response = h.client.get("/api/v1/server-info").send().await;
    response
        .assert_status(StatusCode::OK)
        .assert_header("crisp-api-version", "v1")
        .assert_header("crisp-format-version", "65536");
    assert!(
        response.header("x-request-id").is_some(),
        "request id is stamped"
    );
    let info: ServerInfo = response.json();
    assert_eq!(info.api_versions, vec!["v1"]);
    assert_eq!(info.format_version.current, crisp_core::FORMAT_VERSION);
    assert_eq!(info.core_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(info.platform_keys.len(), 1);
    assert_eq!(info.platform_keys[0].usr_id, platform().user_id());
    assert!(info.platform_keys[0].usr_id.starts_with("usr1"));

    let response = h.client.get("/api/v1/platform-keys").send().await;
    response.assert_status(StatusCode::OK);
    let keys: Vec<Value> = response.json();
    assert_eq!(
        keys[0]["ed25519"],
        hex::encode(&platform().public_keyset().ed25519)
    );

    h.client.assert_conformance();
}

#[tokio::test]
async fn cors_preflight_is_allowed_for_any_origin() {
    // A `POST /bundles` with a bearer token and a non-safelisted content type
    // triggers a browser preflight; the default (lenient) policy must allow it.
    let h = Harness::new();
    let response = h
        .client
        .options("/api/v1/bundles")
        .header("origin", "http://localhost:3000")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "authorization,content-type",
        )
        .send()
        .await;
    assert!(
        response.status().is_success(),
        "preflight: {}",
        response.status()
    );
    let allow_origin = response.header("access-control-allow-origin");
    assert!(
        allow_origin == Some("*") || allow_origin == Some("http://localhost:3000"),
        "any origin should be allowed by default, got {allow_origin:?}"
    );
    assert!(response.header("access-control-allow-methods").is_some());
    let allowed_headers = response
        .header("access-control-allow-headers")
        .unwrap_or_default()
        .to_ascii_lowercase();
    assert!(
        allowed_headers.contains("authorization"),
        "{allowed_headers}"
    );
    assert!(response.header("access-control-max-age").is_some());

    // A real cross-origin response exposes the version headers to browser JS.
    let response = h
        .client
        .get("/api/v1/health")
        .header("origin", "http://localhost:3000")
        .send()
        .await;
    let exposed = response
        .header("access-control-expose-headers")
        .unwrap_or_default()
        .to_ascii_lowercase();
    assert!(exposed.contains("crisp-api-version"), "{exposed}");
}

#[tokio::test]
async fn cors_restricts_to_the_configured_allowlist() {
    let mut config = test_config();
    config.http.allowed_origins = Some(vec!["http://localhost:3000".to_owned()]);
    let h = Harness::with_config(config);

    let response = h
        .client
        .get("/api/v1/server-info")
        .header("origin", "http://localhost:3000")
        .send()
        .await;
    response.assert_header("access-control-allow-origin", "http://localhost:3000");

    let response = h
        .client
        .get("/api/v1/server-info")
        .header("origin", "https://evil.example")
        .send()
        .await;
    assert!(response.header("access-control-allow-origin").is_none());
}

// ---------------------------------------------------------------------------
// Accounts and sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_auth_flow_issues_a_token_that_logout_invalidates() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[1u8; 32]);
    let token = h.register_and_login(&author, "alice").await;
    assert_eq!(token.len(), 64);

    // The token authenticates a logout, which answers 204 with no body.
    let response = logout(&h, &token).await;
    response.assert_status(StatusCode::NO_CONTENT);
    assert!(response.bytes().is_empty());
    assert!(!h.storage.has_session(&token));

    // The token no longer authenticates anything.
    logout(&h, &token)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    let (bundle, _) = build_recipe_bundle(&author);
    h.publish(&token, bundle)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    h.client.assert_conformance();
}

#[tokio::test]
async fn login_rejects_a_signature_from_the_wrong_key() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[2u8; 32]);
    let imposter = SecretKeySet::from_seed(&[99u8; 32]);
    h.register(&author, "bob").await;
    let (nonce, audience) = h.challenge("bob").await;

    let response = h.login(&imposter, &nonce, &audience).await;
    response.assert_status(StatusCode::UNAUTHORIZED);
    assert_code(&response, code::UNAUTHORIZED);

    // The failed attempt consumed the nonce: the real key cannot reuse it.
    h.login(&author, &nonce, &audience)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    h.client.assert_conformance();
}

#[tokio::test]
async fn second_login_with_the_same_nonce_fails() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[20u8; 32]);
    h.register(&author, "replay").await;
    let (nonce, audience) = h.challenge("replay").await;

    h.login(&author, &nonce, &audience)
        .await
        .assert_status(StatusCode::OK);
    let response = h.login(&author, &nonce, &audience).await;
    response.assert_status(StatusCode::UNAUTHORIZED);
    assert_code(&response, code::UNAUTHORIZED);
    h.client.assert_conformance();
}

#[tokio::test]
async fn concurrent_logins_with_the_same_nonce_yield_one_session() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[21u8; 32]);
    h.register(&author, "racer").await;
    let (nonce, audience) = h.challenge("racer").await;

    let (a, b) = tokio::join!(
        h.login(&author, &nonce, &audience),
        h.login(&author, &nonce, &audience)
    );
    let mut statuses = [a.status(), b.status()];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::UNAUTHORIZED]);
    assert_eq!(h.storage.inner.lock().unwrap().sessions.len(), 1);
}

#[tokio::test]
async fn expired_challenge_is_refused() {
    let mut config = test_config();
    config.challenge_ttl_secs = 0; // expires the instant it is issued
    let h = Harness::with_config(config);
    let author = SecretKeySet::from_seed(&[22u8; 32]);
    h.register(&author, "slow").await;
    let (nonce, audience) = h.challenge("slow").await;

    let response = h.login(&author, &nonce, &audience).await;
    response.assert_status(StatusCode::UNAUTHORIZED);
    assert_code(&response, code::UNAUTHORIZED);
}

#[tokio::test]
async fn expired_session_is_refused() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[23u8; 32]);
    let usr_id = h.register(&author, "stale").await;
    h.storage.put_session("stale-token", &usr_id, now() - 1);

    logout(&h, "stale-token")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    // A session that is still valid works, proving the row shape is right.
    h.storage.put_session("fresh-token", &usr_id, now() + 60);
    logout(&h, "fresh-token")
        .await
        .assert_status(StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn challenge_for_an_unknown_username_is_indistinguishable() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[24u8; 32]);
    h.register(&author, "known").await;

    let known = h
        .client
        .post("/api/v1/auth/challenge")
        .json(&json!({ "username": "known" }))
        .send()
        .await;
    let unknown = h
        .client
        .post("/api/v1/auth/challenge")
        .json(&json!({ "username": "nobody" }))
        .send()
        .await;
    known.assert_status(StatusCode::OK);
    unknown.assert_status(StatusCode::OK);
    let known: Value = known.json();
    let unknown: Value = unknown.json();
    // Same shape, no account-revealing fields.
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&known), keys(&unknown));
    assert_eq!(unknown["nonce"].as_str().unwrap().len(), 64);
    // Nothing was stored for the unknown username, so its nonce can never
    // validate — not even with a valid signature from some real key.
    assert_eq!(h.storage.inner.lock().unwrap().challenges.len(), 1);
    let response = h
        .login(
            &author,
            unknown["nonce"].as_str().unwrap(),
            unknown["audience"].as_str().unwrap(),
        )
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
    h.client.assert_conformance();
}

#[tokio::test]
async fn account_profile_endpoint_returns_a_verifiable_profile() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[6u8; 32]);
    let response = h
        .client
        .post("/api/v1/auth/register")
        .json(&json!({ "profile": signed_profile(&author, "erin", "Erin Baker") }))
        .send()
        .await;
    response.assert_status(StatusCode::OK);
    let usr_id = response.json::<Value>()["usr_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let response = h
        .client
        .get(&format!("/api/v1/accounts/{usr_id}"))
        .send()
        .await;
    response
        .assert_status(StatusCode::OK)
        .assert_header("content-type", "application/octet-stream");

    // The served bytes are a signed profile that verifies to the same fields.
    let profile =
        crisp_core::profile::verify_profile(response.bytes()).expect("verify served profile");
    assert_eq!(profile.username, "erin");
    assert_eq!(profile.display_name, "Erin Baker");
    assert_eq!(profile.keyset, author.public_keyset());
    h.client.assert_conformance();
}

#[tokio::test]
async fn account_profile_endpoint_404s_for_an_unknown_id() {
    let h = Harness::new();
    let response = h.client.get("/api/v1/accounts/usr1nobody").send().await;
    response.assert_status(StatusCode::NOT_FOUND);
    assert_code(&response, code::NOT_FOUND);
}

#[tokio::test]
async fn register_rejects_a_tampered_profile() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[7u8; 32]);
    let mut bytes = hex::decode(signed_profile(&author, "frank", "Frank")).expect("hex");
    // Flip a byte; the composite signature must no longer verify.
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    let response = h
        .client
        .post("/api/v1/auth/register")
        .json(&json!({ "profile": hex::encode(&bytes) }))
        .send()
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert_code(&response, code::BAD_REQUEST);
}

#[tokio::test]
async fn register_reports_distinct_conflicts_for_a_name_and_a_key() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[8u8; 32]);
    let other = SecretKeySet::from_seed(&[88u8; 32]);
    h.register(&author, "grace").await;

    let register = |profile: String| {
        h.client
            .post("/api/v1/auth/register")
            .json(&json!({ "profile": profile }))
            .send()
    };

    // Re-registering the same key (a returning user) is a key conflict.
    let response = register(signed_profile(&author, "grace", "Grace")).await;
    response.assert_status(StatusCode::CONFLICT);
    assert_code(&response, code::CONFLICT);
    let detail = response.json::<Value>()["detail"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(detail.contains("key"), "expected a key conflict: {detail}");

    // A different key claiming the taken name is a username conflict.
    let response = register(signed_profile(&other, "grace", "Impostor")).await;
    response.assert_status(StatusCode::CONFLICT);
    let detail = response.json::<Value>()["detail"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        detail.contains("username"),
        "expected a username conflict: {detail}"
    );
    h.client.assert_conformance();
}

#[tokio::test]
async fn register_race_lost_at_the_unique_constraint_is_a_conflict() {
    let h = Harness::new();
    // The pre-checks pass, but the insert loses to a concurrent registration.
    h.storage.inject("create_account", Fault::Duplicate);
    let response = h
        .client
        .post("/api/v1/auth/register")
        .json(&json!({ "profile": signed_profile(&SecretKeySet::from_seed(&[9u8; 32]), "heidi", "H") }))
        .send()
        .await;
    response.assert_status(StatusCode::CONFLICT);
    assert_code(&response, code::CONFLICT);
}

// ---------------------------------------------------------------------------
// Publication and retrieval
// ---------------------------------------------------------------------------

#[tokio::test]
async fn publish_endorses_indexes_and_serves_a_bundle() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[3u8; 32]);
    let token = h.register_and_login(&author, "carol").await;
    let (bundle_bytes, doc_id, revision) =
        build_recipe_bundle_with(&author, &RecipeSpec::default());

    let response = h.publish(&token, bundle_bytes).await;
    response
        .assert_status(StatusCode::CREATED)
        .assert_header("content-type", "application/octet-stream")
        .assert_header("location", &format!("/api/v1/bundles/{doc_id}/{revision}"));
    let endorsed = response.bytes().to_vec();

    // The returned bytes are endorsed and Published-compliant.
    let report = verify(
        &endorsed,
        &platform().platform_keys(),
        Timestamp(now() as u64),
    );
    assert!(report.compliant, "endorsed bundle should be compliant");
    assert_eq!(report.trust, Trust::Endorsed);

    // Retrieval by doc id returns the stored endorsed bytes.
    let response = h
        .client
        .get(&format!("/api/v1/bundles/{doc_id}"))
        .send()
        .await;
    response.assert_status(StatusCode::OK);
    assert_eq!(response.bytes().as_ref(), endorsed.as_slice());

    // The recipe is now searchable and browsable, with typed metadata.
    let response = h.client.get("/api/v1/search?q=cookies").send().await;
    response.assert_status(StatusCode::OK);
    let page: RecipePage = response.json();
    assert_eq!(page.recipes.len(), 1);
    let summary = &page.recipes[0];
    assert_eq!(summary.doc_id, doc_id);
    assert_eq!(summary.cuisine, vec!["american"]);
    assert_eq!(summary.course, vec!["dessert"]);
    assert_eq!(summary.allergens, vec!["gluten"]);
    assert_eq!(summary.authors[0].name, "Tester");

    let page: RecipePage = h.client.get("/api/v1/recipes").send().await.json();
    assert_eq!(page.recipes.len(), 1);
    assert_eq!(page.limit, 50);

    h.client.assert_conformance();
}

#[tokio::test]
async fn exact_revision_fetch_distinguishes_revisions() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[30u8; 32]);
    let token = h.register_and_login(&author, "ivan").await;

    let first = RecipeSpec::default();
    let second = RecipeSpec {
        modified_at: 2_500,
        ..RecipeSpec::default()
    };
    let (v1, doc_id, rev1) = build_recipe_bundle_with(&author, &first);
    let (v2, doc_id2, rev2) = build_recipe_bundle_with(&author, &second);
    assert_eq!(doc_id, doc_id2, "same identity, same document");
    assert_ne!(rev1, rev2, "different content, different revision");

    let endorsed1 = h.publish(&token, v1).await.bytes().to_vec();
    // Make the second publication strictly later.
    h.storage.inner.lock().unwrap().bundles[0].published_at -= 10;
    let endorsed2 = h.publish(&token, v2).await.bytes().to_vec();

    let exact1 = h
        .client
        .get(&format!("/api/v1/bundles/{doc_id}/{rev1}"))
        .send()
        .await;
    exact1.assert_status(StatusCode::OK);
    assert_eq!(exact1.bytes().as_ref(), endorsed1.as_slice());
    let exact2 = h
        .client
        .get(&format!("/api/v1/bundles/{doc_id}/{rev2}"))
        .send()
        .await;
    assert_eq!(exact2.bytes().as_ref(), endorsed2.as_slice());
    let latest = h
        .client
        .get(&format!("/api/v1/bundles/{doc_id}"))
        .send()
        .await;
    assert_eq!(latest.bytes().as_ref(), endorsed2.as_slice());

    let missing = h
        .client
        .get(&format!("/api/v1/bundles/{doc_id}/rev1nope"))
        .send()
        .await;
    missing.assert_status(StatusCode::NOT_FOUND);
    assert_code(&missing, code::NOT_FOUND);
    h.client.assert_conformance();
}

#[tokio::test]
async fn publish_requires_authentication() {
    let h = Harness::new();
    let (bundle_bytes, _) = build_recipe_bundle(&SecretKeySet::from_seed(&[4u8; 32]));
    let response = h
        .client
        .post("/api/v1/bundles")
        .body("application/octet-stream", bundle_bytes)
        .send()
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
    assert!(response.header("www-authenticate").is_some());
}

#[tokio::test]
async fn publish_rejects_a_non_compliant_bundle_with_its_report() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[5u8; 32]);
    let token = h.register_and_login(&author, "dave").await;

    let response = h.publish(&token, vec![0x00, 0x01, 0x02]).await;
    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_code(&response, code::VERIFICATION_FAILED);
    let body: Value = response.json();
    assert_eq!(body["report"]["compliant"], false);
    assert!(
        body["report"]["checks"]
            .as_array()
            .is_some_and(|c| !c.is_empty())
    );
    h.client.assert_conformance();
}

#[tokio::test]
async fn publish_rejects_a_bundle_by_another_author() {
    let h = Harness::new();
    let carol = SecretKeySet::from_seed(&[31u8; 32]);
    let dave = SecretKeySet::from_seed(&[32u8; 32]);
    let token = h.register_and_login(&carol, "carol2").await;
    let (daves_bundle, _) = build_recipe_bundle(&dave);

    let response = h.publish(&token, daves_bundle).await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert_code(&response, code::BAD_REQUEST);
    assert!(h.storage.inner.lock().unwrap().bundles.is_empty());
}

#[tokio::test]
async fn duplicate_revision_is_a_conflict() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[33u8; 32]);
    let token = h.register_and_login(&author, "judy").await;
    let (bundle_bytes, _) = build_recipe_bundle(&author);

    h.publish(&token, bundle_bytes.clone())
        .await
        .assert_status(StatusCode::CREATED);
    let response = h.publish(&token, bundle_bytes).await;
    response.assert_status(StatusCode::CONFLICT);
    assert_code(&response, code::CONFLICT);
    assert_eq!(h.storage.inner.lock().unwrap().bundles.len(), 1);
}

#[tokio::test]
async fn publish_race_lost_at_the_unique_constraint_is_a_conflict() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[34u8; 32]);
    let token = h.register_and_login(&author, "ken").await;
    h.storage.inject("insert_bundle", Fault::Duplicate);
    let (bundle_bytes, _) = build_recipe_bundle(&author);
    let response = h.publish(&token, bundle_bytes).await;
    response.assert_status(StatusCode::CONFLICT);
    assert_code(&response, code::CONFLICT);
}

#[tokio::test]
async fn oversize_upload_is_refused_with_413() {
    let mut config = test_config();
    config.http = HttpOptions {
        max_bundle_bytes: 1024,
        ..HttpOptions::default()
    };
    let h = Harness::with_config(config);
    let author = SecretKeySet::from_seed(&[35u8; 32]);
    let token = h.register_and_login(&author, "leo").await;
    let (bundle_bytes, _) = build_recipe_bundle(&author);
    assert!(bundle_bytes.len() > 1024);

    let response = h.publish(&token, bundle_bytes).await;
    response.assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    assert!(h.storage.inner.lock().unwrap().bundles.is_empty());
    h.client.assert_conformance();
}

#[tokio::test]
async fn pre_endorsed_uploads_are_rejected() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[36u8; 32]);
    let token = h.register_and_login(&author, "mallory").await;
    let (bundle_bytes, _) = build_recipe_bundle(&author);

    // Endorsed by this very platform: would otherwise be double-endorsed.
    let ours = endorse_with(PLATFORM_SEED, &bundle_bytes);
    let response = h.publish(&token, ours).await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert_code(&response, code::BAD_REQUEST);

    // Endorsed by some other platform key.
    let foreign = endorse_with([77u8; 32], &bundle_bytes);
    let response = h.publish(&token, foreign).await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert_code(&response, code::BAD_REQUEST);

    assert!(h.storage.inner.lock().unwrap().bundles.is_empty());
    h.client.assert_conformance();
}

#[tokio::test]
async fn publish_requires_the_octet_stream_media_type() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[37u8; 32]);
    let token = h.register_and_login(&author, "nina").await;
    let (bundle_bytes, _) = build_recipe_bundle(&author);
    h.client
        .post("/api/v1/bundles")
        .header("authorization", &format!("Bearer {token}"))
        .body("application/crisp+cbor", bundle_bytes)
        .send()
        .await
        .assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_facets_are_comma_separated() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[40u8; 32]);
    let token = h.register_and_login(&author, "olga").await;
    for (title, cuisine) in [
        ("Tacos", vec!["mexican", "street"]),
        ("Pho", vec!["vietnamese", "street"]),
        ("Pie", vec!["american"]),
    ] {
        let spec = RecipeSpec {
            title: title.into(),
            cuisine: cuisine.into_iter().map(str::to_owned).collect(),
            ..RecipeSpec::default()
        };
        let (bytes, _, _) = build_recipe_bundle_with(&author, &spec);
        h.publish(&token, bytes)
            .await
            .assert_status(StatusCode::CREATED);
    }

    let titles = |page: RecipePage| {
        let mut t: Vec<String> = page.recipes.into_iter().map(|r| r.title).collect();
        t.sort();
        t
    };
    let page: RecipePage = h
        .client
        .get("/api/v1/search?cuisine=street")
        .send()
        .await
        .json();
    assert_eq!(titles(page), vec!["Pho", "Tacos"]);
    let page: RecipePage = h
        .client
        .get("/api/v1/search?cuisine=street,mexican")
        .send()
        .await
        .json();
    assert_eq!(titles(page), vec!["Tacos"]);
    let page: RecipePage = h
        .client
        .get("/api/v1/search?allergen_free=gluten")
        .send()
        .await
        .json();
    assert!(page.recipes.is_empty());

    let page: RecipePage = h
        .client
        .get("/api/v1/recipes?limit=2&offset=1")
        .send()
        .await
        .json();
    assert_eq!((page.limit, page.offset, page.recipes.len()), (2, 1, 2));

    let response = h.client.get("/api/v1/recipes?limit=0").send().await;
    response.assert_status(StatusCode::BAD_REQUEST);
    h.client.assert_conformance();
}

// ---------------------------------------------------------------------------
// The contract: every declared response is reachable
// ---------------------------------------------------------------------------

/// Statuses kynos declares that this API can never produce, with the reason.
///
/// `Auth<S>` always declares 403 for an authorizer that rejects scopes; Crisp
/// defines no scopes, so its authorizer never answers 403.
const UNREACHABLE: &[(&str, &str, &str)] = &[
    ("/api/v1/auth/logout", "post", "403"),
    ("/api/v1/bundles", "post", "403"),
];

/// Sends the four JSON body rejections kynos declares on a JSON operation:
/// 400 (syntax), 413 (over the 2 MiB default), 415 (media type), 422 (schema).
async fn json_rejections(client: &TestClient<crisp_server::state::App>, path: &str) {
    client
        .post(path)
        .body("application/json", "{not json")
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    let huge = format!("{{\"pad\":\"{}\"}}", "x".repeat(3 * 1024 * 1024));
    client
        .post(path)
        .body("application/json", huge)
        .send()
        .await
        .assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    client
        .post(path)
        .body("text/plain", "hello")
        .send()
        .await
        .assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    client
        .post(path)
        .json(&json!({ "unexpected": 1 }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn every_declared_response_is_exercised() {
    // Boxed: `#[tokio::test]` pins the test future on the thread's stack, and
    // this tour's state machine is large enough that, with unoptimized ML-DSA
    // signing on top, it would overflow it.
    Box::pin(declared_response_tour()).await;
}

/// Drives every operation through every response it declares.
async fn declared_response_tour() {
    let coverage = Coverage::new();
    let mut config = test_config();
    config.http.max_bundle_bytes = 256 * 1024;
    let storage = Arc::new(MockStorage::default());
    let h = Harness::covered(config.clone(), Arc::clone(&storage), &coverage);
    let c = &h.client;
    let author = SecretKeySet::from_seed(&[50u8; 32]);
    let stranger = SecretKeySet::from_seed(&[51u8; 32]);

    // Liveness and version endpoints.
    for path in [
        "/api/v1/health",
        "/api/v1/server-info",
        "/api/v1/platform-keys",
    ] {
        c.get(path).send().await.assert_status(StatusCode::OK);
    }

    // Register: 200, 409, 400, JSON rejections, 500.
    let usr_id = h.register(&author, "tour").await;
    let response = c
        .post("/api/v1/auth/register")
        .json(&json!({ "profile": signed_profile(&author, "tour", "Tour") }))
        .send()
        .await;
    response.assert_status(StatusCode::CONFLICT);
    c.post("/api/v1/auth/register")
        .json(&json!({ "profile": "zz" }))
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    json_rejections(c, "/api/v1/auth/register").await;
    storage.inject("account_by_usr_id", Fault::Db);
    let response = c
        .post("/api/v1/auth/register")
        .json(&json!({ "profile": signed_profile(&stranger, "x", "X") }))
        .send()
        .await;
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    assert_code(&response, code::INTERNAL);
    storage.heal();

    // Challenge: 200, JSON rejections, 500.
    let (nonce, audience) = h.challenge("tour").await;
    json_rejections(c, "/api/v1/auth/challenge").await;
    storage.inject("account_by_username", Fault::Db);
    c.post("/api/v1/auth/challenge")
        .json(&json!({ "username": "tour" }))
        .send()
        .await
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    storage.heal();

    // Login: 200, 401 (replay), 400 (bad hex), JSON rejections, 500.
    let token = h.login(&author, &nonce, &audience).await.json::<Value>()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    h.login(&author, &nonce, &audience)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    let (nonce2, _) = h.challenge("tour").await;
    c.post("/api/v1/auth/login")
        .json(&json!({ "nonce": nonce2, "signature": "zz" }))
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    json_rejections(c, "/api/v1/auth/login").await;
    storage.inject("take_challenge", Fault::Db);
    c.post("/api/v1/auth/login")
        .json(&json!({ "nonce": "n", "signature": "00" }))
        .send()
        .await
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    storage.heal();

    // Account profile: 200, 404, 400 (undecodable path), 500.
    let account = format!("/api/v1/accounts/{usr_id}");
    c.get(&account).send().await.assert_status(StatusCode::OK);
    c.get("/api/v1/accounts/usr1none")
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
    c.get("/api/v1/accounts/%FF")
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    storage.inject("account_by_usr_id", Fault::Db);
    c.get(&account)
        .send()
        .await
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    storage.heal();

    // Publish: 201, 409, 400, 401, 413, 415, 422, 500.
    let (bundle_bytes, doc_id, revision) =
        build_recipe_bundle_with(&author, &RecipeSpec::default());
    h.publish(&token, bundle_bytes.clone())
        .await
        .assert_status(StatusCode::CREATED);
    h.publish(&token, bundle_bytes)
        .await
        .assert_status(StatusCode::CONFLICT);
    h.publish(&token, Vec::new())
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    h.publish("not-a-token", vec![1])
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    h.publish(&token, vec![0u8; 300 * 1024])
        .await
        .assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    c.post("/api/v1/bundles")
        .header("authorization", &format!("Bearer {token}"))
        .body("text/plain", "hello")
        .send()
        .await
        .assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    h.publish(&token, vec![0x00, 0x01])
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    storage.inject("insert_bundle", Fault::Db);
    let (other, _, _) = build_recipe_bundle_with(
        &author,
        &RecipeSpec {
            modified_at: 2_100,
            ..RecipeSpec::default()
        },
    );
    h.publish(&token, other)
        .await
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    storage.heal();

    // Retrieval: 200, 404, 400, 500 for latest and exact.
    for (path, op) in [
        (format!("/api/v1/bundles/{doc_id}"), "bundle_latest"),
        (
            format!("/api/v1/bundles/{doc_id}/{revision}"),
            "bundle_exact",
        ),
    ] {
        c.get(&path).send().await.assert_status(StatusCode::OK);
        c.get(&format!("{path}x"))
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND);
        c.get(&format!("{path}%FF"))
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);
        storage.inject(op, Fault::Db);
        c.get(&path)
            .send()
            .await
            .assert_status(StatusCode::INTERNAL_SERVER_ERROR);
        storage.heal();
    }

    // Browse and search: 200, 400, 500.
    for (path, op) in [("/api/v1/recipes", "browse"), ("/api/v1/search", "search")] {
        c.get(path).send().await.assert_status(StatusCode::OK);
        c.get(&format!("{path}?limit=abc"))
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);
        storage.inject(op, Fault::Db);
        c.get(path)
            .send()
            .await
            .assert_status(StatusCode::INTERNAL_SERVER_ERROR);
        storage.heal();
    }

    // Logout: 204, 401, 500 (last, since it ends the session).
    storage.inject("delete_session", Fault::Db);
    logout(&h, &token)
        .await
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    storage.heal();
    logout(&h, &token)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    logout(&h, &token)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    c.assert_conformance();

    // Every declared response was produced, except the documented
    // unreachable ones, which are removed from the document checked here.
    let document = crisp_server::router(&config.http).openapi().unwrap();
    let mut value = serde_json::to_value(&document).unwrap();
    for (path, method, status) in UNREACHABLE {
        let removed = value["paths"][path][method]["responses"]
            .as_object_mut()
            .and_then(|responses| responses.remove(*status));
        assert!(
            removed.is_some(),
            "{method} {path} no longer declares {status}"
        );
    }
    let document = serde_json::from_value(value).unwrap();
    coverage.assert_declared_responses_covered(&document);
}
