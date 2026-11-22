//! Shared steps of the declared-response tour: responses every JSON
//! operation declares, the 429 every throttled operation declares, and the
//! probes and 503 every database-backed operation declares.

#![allow(dead_code)] // Each test binary uses a different subset.

pub mod deletion;
pub mod moderation;
pub mod outage;
pub mod quota;
pub mod sessions;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use crisp_core::sign::SecretKeySet;
use crisp_server::config::Config;
use crisp_server::error::code;
use crisp_server::limits::{Budget, RateLimits};
use crisp_server::state::App;
use kynos::http::StatusCode;
use kynos::test::{Coverage, TestClient, TestRequest, TestResponse};
use serde_json::json;

use crate::common::{Harness, MockStorage, assert_code, signed_profile, test_config};

/// Statuses kynos declares that this API can never produce, with the reason.
///
/// The router recovers panics, which declares a 500 on every operation. The
/// four below have no failure of their own, so only a panic answers 500;
/// `tests/panics.rs` drives that recovery.
pub const UNREACHABLE: &[(&str, &str, &str)] = &[
    ("/api/v1/health", "get", "500"),
    ("/api/v1/ready", "get", "500"),
    ("/api/v1/server-info", "get", "500"),
    ("/api/v1/platform-keys", "get", "500"),
];

/// Sends `request` with a `media_type` body that declares one octet and never
/// sends it, and asserts the 408 kynos declares on every operation reading a
/// body. The in-process client gives up on the stalled read at once, so this
/// proves the response is routed and declared; `tests/http_layer.rs` proves
/// the configured idle timeout through the real server.
pub async fn assert_stalled_body_times_out(request: TestRequest<'_, App>, media_type: &str) {
    request
        .stalled_body(media_type)
        .send()
        .await
        .assert_status(StatusCode::REQUEST_TIMEOUT);
}

/// Sends the five body rejections kynos declares on a JSON operation:
/// 400 (syntax), 408 (stalled), 413 (over the 2 MiB default), 415 (media
/// type), 422 (schema).
pub async fn json_rejections(client: &TestClient<App>, path: &str) {
    client
        .post(path)
        .body("application/json", "{not json")
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    assert_stalled_body_times_out(client.post(path), "application/json").await;
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

/// A test configuration in which every throttled operation allows `requests`
/// per hour.
pub fn throttled_config(requests: u32) -> Config {
    let budget = Budget::new(
        NonZeroU32::new(requests).expect("a positive budget"),
        Duration::from_secs(60 * 60),
    );
    let mut config = test_config();
    config.http.limits = RateLimits {
        register: budget,
        challenge: budget,
        challenge_peer: budget,
        login: budget,
        publish: budget,
        query: budget,
    };
    config
}

/// Asserts `response` is the rate-limit refusal: 429, problem+json with the
/// stable `rate_limited` code, and a `Retry-After` in whole seconds.
pub fn assert_throttled(response: &TestResponse) {
    response.assert_status(StatusCode::TOO_MANY_REQUESTS);
    assert_code(response, code::RATE_LIMITED);
    let retry_after: u64 = response
        .header("retry-after")
        .expect("a Retry-After header")
        .parse()
        .expect("Retry-After is whole seconds");
    assert!(
        (1..=3600).contains(&retry_after),
        "Retry-After {retry_after}s is outside the one-hour window"
    );
}

/// Under budgets of one request per hour, spends each throttled operation's
/// budget once and asserts the next request is refused with 429.
pub async fn throttled_responses(coverage: &Coverage) {
    let h = Harness::covered(
        throttled_config(1),
        Arc::new(MockStorage::default()),
        coverage,
    );
    let author = SecretKeySet::from_seed(&[60u8; 32]);
    let token = h.register_and_login(&author, "throttled").await;
    let profile = json!({ "profile": signed_profile(&author, "other", "Other") });
    let register = h.client.post("/api/v1/auth/register").json(&profile);
    assert_throttled(&register.send().await);
    let challenge = json!({ "username": "throttled" });
    let response = h.client.post("/api/v1/auth/challenge").json(&challenge);
    assert_throttled(&response.send().await);
    let login = json!({ "nonce": "00", "signature": "00" });
    let response = h.client.post("/api/v1/auth/login").json(&login);
    assert_throttled(&response.send().await);

    // A refused upload still spent the budget: the work was done.
    let first = h.publish(&token, vec![0x00]).await;
    first.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_throttled(&h.publish(&token, vec![0x00]).await);

    // Browse, search, and the revocation list share one budget.
    h.client
        .get("/api/v1/recipes")
        .send()
        .await
        .assert_status(StatusCode::OK);
    assert_throttled(&h.client.get("/api/v1/search").send().await);
    assert_throttled(&h.client.get("/api/v1/recipes").send().await);
    assert_throttled(&h.client.get("/api/v1/revocations").send().await);
}
