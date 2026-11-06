//! Panic recovery: a handler that panics is answered with an RFC 9457 500
//! that carries none of the panic's payload, and the server keeps serving.
//!
//! The panic is raised inside a real handler's call path by the mock
//! storage, so this drives the production router exactly as `main` builds it.

mod common;

use common::{Fault, Harness, PANIC_MESSAGE};
use kynos::http::StatusCode;
use serde_json::Value;

#[tokio::test]
async fn a_panicking_handler_is_answered_with_a_bare_500_problem() {
    let h = Harness::new();
    h.storage.inject("account_by_usr_id", Fault::Panic);

    let response = h.client.get("/api/v1/accounts/usr1nobody").send().await;

    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        response.header("content-type"),
        Some("application/problem+json")
    );
    let problem: Value = response.json();
    assert_eq!(problem["status"], 500);
    assert!(
        !response.text().contains(PANIC_MESSAGE),
        "the 500 leaks the panic payload: {}",
        response.text()
    );

    // The same server answers the next request normally.
    h.storage.heal();
    h.client
        .get("/api/v1/accounts/usr1nobody")
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
