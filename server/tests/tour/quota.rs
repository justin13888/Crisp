//! The storage-quota responses: 403 `quota_exceeded` and 507 `storage_full`
//! from `POST /bundles`, and the responses `GET /account/usage` declares.

use std::sync::Arc;

use crisp_core::sign::SecretKeySet;
use crisp_server::config::Config;
use crisp_server::error::code;
use crisp_server::quota::{AccountUsage, Quota};
use kynos::http::StatusCode;
use kynos::test::{Coverage, TestResponse};
use serde_json::Value;

use crate::common::{
    Fault, Harness, MockStorage, RecipeSpec, assert_code, build_recipe_bundle_with, test_config,
};

/// A test configuration enforcing `quota`.
pub fn quota_config(quota: Quota) -> Config {
    Config {
        quota,
        ..test_config()
    }
}

/// A distinct recipe bundle by `author`: each `n` is its own revision.
pub fn revision(author: &SecretKeySet, n: u64) -> Vec<u8> {
    let spec = RecipeSpec {
        modified_at: 2_000 + n,
        ..RecipeSpec::default()
    };
    build_recipe_bundle_with(author, &spec).0
}

/// Sends an authenticated `GET /account/usage`.
pub async fn usage(h: &Harness, token: &str) -> TestResponse {
    h.client
        .get("/api/v1/account/usage")
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await
}

/// Asserts `response` is a `quota_exceeded` refusal of `quota` and returns
/// its body.
pub fn assert_quota_exceeded(response: &TestResponse, quota: &str) -> Value {
    response.assert_status(StatusCode::FORBIDDEN);
    assert_code(response, code::QUOTA_EXCEEDED);
    let body: Value = response.json();
    assert_eq!(body["type"], "urn:crisp:problem:quota-exceeded");
    assert_eq!(body["quota"], quota);
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("delete old revisions"), "{detail}");
    body
}

/// Drives every quota response: usage before and after a publish, both
/// per-account refusals, the server cap, and usage's 401 and 500.
pub async fn quota_responses(coverage: &Coverage) {
    let author = SecretKeySet::from_seed(&[80u8; 32]);

    // One bundle per account: the first fits, and usage counts it.
    let storage = Arc::new(MockStorage::default());
    let limits = Quota {
        bundles_per_account: 1,
        ..Quota::default()
    };
    let h = Harness::covered(quota_config(limits), Arc::clone(&storage), coverage);
    let token = h.register_and_login(&author, "quota").await;
    let before = usage(&h, &token).await;
    before.assert_status(StatusCode::OK);
    let before: AccountUsage = before.json();
    assert_eq!(
        before,
        AccountUsage {
            bytes_used: 0,
            bytes_limit: limits.bytes_per_account,
            bundles_used: 0,
            bundles_limit: 1,
        }
    );
    let published = h.publish(&token, revision(&author, 0)).await;
    published.assert_status(StatusCode::CREATED);
    let size = published.bytes().len() as u64;
    let after: AccountUsage = usage(&h, &token).await.json();
    assert_eq!((after.bytes_used, after.bundles_used), (size, 1));

    // A second bundle crosses the bundle quota and is not stored.
    let refused = h.publish(&token, revision(&author, 1)).await;
    let body = assert_quota_exceeded(&refused, "bundles");
    assert_eq!(
        (&body["limit"], &body["used"], &body["requested"]),
        (&1.into(), &1.into(), &1.into())
    );
    assert_eq!(usage(&h, &token).await.json::<AccountUsage>(), after);

    // Usage: 401 without a session, 500 when the sum fails.
    h.client
        .get("/api/v1/account/usage")
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    storage.inject("account_usage", Fault::Db);
    let failed = usage(&h, &token).await;
    failed.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    assert_code(&failed, code::INTERNAL);
    storage.heal();

    // A byte quota smaller than the bundle refuses it, naming bytes.
    let tiny = Quota {
        bytes_per_account: 1,
        ..Quota::default()
    };
    let h = Harness::covered(quota_config(tiny), Arc::default(), coverage);
    let token = h.register_and_login(&author, "quota").await;
    let refused = h.publish(&token, revision(&author, 0)).await;
    let body = assert_quota_exceeded(&refused, "bytes");
    assert_eq!((&body["limit"], &body["used"]), (&1.into(), &0.into()));
    assert!(body["requested"].as_u64().unwrap_or_default() > 1);

    // A server cap smaller than the bundle is 507, and says nothing about
    // what other accounts store.
    let full = Quota {
        max_stored_bytes: Some(1),
        ..Quota::default()
    };
    let h = Harness::covered(quota_config(full), Arc::default(), coverage);
    let token = h.register_and_login(&author, "quota").await;
    let refused = h.publish(&token, revision(&author, 0)).await;
    refused.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    assert_code(&refused, code::STORAGE_FULL);
    let body: Value = refused.json();
    assert_eq!(body["type"], "urn:crisp:problem:storage-full");
    assert!(body.get("used").is_none() && body.get("limit").is_none());
    h.client.assert_conformance();
}
