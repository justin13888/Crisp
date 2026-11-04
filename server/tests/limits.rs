//! Per-operation abuse limits, driven through the real router: each budget's
//! key, the 429 it answers with, and the pagination offset cap.

mod common;
mod tour;

use std::num::NonZeroU32;
use std::time::Duration;

use common::{Harness, build_recipe_bundle};
use crisp_core::sign::SecretKeySet;
use crisp_server::limits::Budget;
use crisp_server::search::MAX_OFFSET;
use kynos::http::StatusCode;
use kynos::test::Coverage;
use serde_json::json;
use tour::{assert_throttled, throttled_config, throttled_responses};

#[tokio::test]
async fn every_throttled_operation_answers_429_with_retry_after() {
    Box::pin(throttled_responses(&Coverage::new())).await;
}

#[tokio::test]
async fn challenges_are_counted_per_username() {
    let h = Harness::with_config(throttled_config(1));
    h.challenge("alice").await;
    let again = h
        .client
        .post("/api/v1/auth/challenge")
        .json(&json!({ "username": "alice" }))
        .send()
        .await;
    assert_throttled(&again);
    h.challenge("bob").await;
}

#[tokio::test]
async fn publishes_are_counted_per_account() {
    let mut config = throttled_config(1);
    let plenty = Budget::new(NonZeroU32::new(10).unwrap(), Duration::from_secs(3600));
    config.http.limits.register = plenty;
    config.http.limits.challenge = plenty;
    config.http.limits.login = plenty;
    let h = Harness::with_config(config);
    let alice = SecretKeySet::from_seed(&[70u8; 32]);
    let bob = SecretKeySet::from_seed(&[71u8; 32]);
    let alice_token = h.register_and_login(&alice, "alice").await;
    let bob_token = h.register_and_login(&bob, "bob").await;

    let (bundle, _) = build_recipe_bundle(&alice);
    h.publish(&alice_token, bundle)
        .await
        .assert_status(StatusCode::CREATED);
    let (bundle, _) = build_recipe_bundle(&alice);
    assert_throttled(&h.publish(&alice_token, bundle).await);
    let (bundle, _) = build_recipe_bundle(&bob);
    h.publish(&bob_token, bundle)
        .await
        .assert_status(StatusCode::CREATED);
}

#[tokio::test]
async fn an_unauthenticated_publish_is_refused_before_it_is_counted() {
    let h = Harness::with_config(throttled_config(1));
    for _ in 0..2 {
        h.publish("not-a-token", vec![0x00])
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn pagination_offsets_are_capped() {
    let h = Harness::new();
    for path in ["/api/v1/recipes", "/api/v1/search"] {
        h.client
            .get(&format!("{path}?offset={MAX_OFFSET}"))
            .send()
            .await
            .assert_status(StatusCode::OK);
        let response = h
            .client
            .get(&format!("{path}?offset={}", MAX_OFFSET + 1))
            .send()
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
    }
}
