//! The responses `POST /auth/logout-all` and `GET /auth/sessions` declare,
//! and the helpers that send them.

use std::sync::Arc;

use crisp_core::sign::SecretKeySet;
use crisp_server::error::code;
use kynos::http::StatusCode;
use kynos::test::{Coverage, TestResponse};
use serde_json::Value;

use crate::common::{Fault, Harness, MockStorage, assert_code, test_config};

/// Sends an authenticated `POST /auth/logout-all`.
pub async fn logout_all(h: &Harness, token: &str) -> TestResponse {
    h.client
        .post("/api/v1/auth/logout-all")
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await
}

/// Sends an authenticated `GET /auth/sessions`.
pub async fn list_sessions(h: &Harness, token: &str) -> TestResponse {
    h.client
        .get("/api/v1/auth/sessions")
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await
}

/// Opens one more session for an already-registered `username`.
pub async fn second_session(h: &Harness, author: &SecretKeySet, username: &str) -> String {
    let (nonce, audience) = h.challenge(username).await;
    let response = h.login(author, &nonce, &audience).await;
    response.assert_status(StatusCode::OK);
    response.json::<Value>()["token"]
        .as_str()
        .expect("a token")
        .to_owned()
}

/// Drives both operations through 200/204, 401, and 500; their 503 is in
/// [`super::outage`] and their 403 is [`super::UNREACHABLE`].
pub async fn session_responses(coverage: &Coverage) {
    let storage = Arc::new(MockStorage::default());
    let h = Harness::covered(test_config(), Arc::clone(&storage), coverage);
    let author = SecretKeySet::from_seed(&[90u8; 32]);
    let token = h.register_and_login(&author, "sessions").await;

    list_sessions(&h, &token)
        .await
        .assert_status(StatusCode::OK);
    storage.inject("sessions_for", Fault::Db);
    let response = list_sessions(&h, &token).await;
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    assert_code(&response, code::INTERNAL);
    storage.inject("delete_sessions_for", Fault::Db);
    let response = logout_all(&h, &token).await;
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    assert_code(&response, code::INTERNAL);
    storage.heal();

    logout_all(&h, &token)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    logout_all(&h, &token)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    list_sessions(&h, &token)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    h.client.assert_conformance();
}
