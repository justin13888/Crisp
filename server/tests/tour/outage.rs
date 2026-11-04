//! The probes and the 503 every database-backed operation declares: a
//! server whose database is down keeps answering liveness and version
//! requests, fails readiness, and answers every operation that needs the
//! database with an opaque 503 problem - never 401, never 500.

use std::sync::Arc;

use async_trait::async_trait;
use crisp_core::sign::SecretKeySet;
use crisp_server::db::{
    account, bundle, challenge, moderation_action, recipe_index, report, session, suspension,
    takedown,
};
use crisp_server::error::code;
use crisp_server::quota::{Quota, Usage};
use crisp_server::state::App;
use crisp_server::storage::{
    BundleInsertError, FileReport, InsertError, Lift, Moderation, NewReport, NewSuspension,
    NewTakedown, PurgeCounts, Resolve, SearchQuery, Storage, Suspend, TakeDown, Unpublish,
};
use kynos::http::StatusCode;
use kynos::test::{Coverage, TestClient, TestResponse};
use sea_orm::{ConnAcquireErr, DbErr};
use serde_json::json;

use crate::common::{
    Fault, Harness, MockStorage, assert_code, platform, signed_profile, test_config,
};

/// A [`Storage`] whose database is unreachable: every call fails the way an
/// exhausted or disconnected pool does.
struct DownStorage;

/// The error a pool reports when no connection can be had in time.
fn down() -> DbErr {
    DbErr::ConnectionAcquire(ConnAcquireErr::Timeout)
}

#[async_trait]
impl Storage for DownStorage {
    async fn ping(&self) -> Result<(), DbErr> {
        Err(down())
    }
    async fn create_account(&self, _: account::ActiveModel) -> Result<(), InsertError> {
        Err(down().into())
    }
    async fn account_by_username(&self, _: &str) -> Result<Option<account::Model>, DbErr> {
        Err(down())
    }
    async fn account_by_usr_id(&self, _: &str) -> Result<Option<account::Model>, DbErr> {
        Err(down())
    }
    async fn create_challenge(&self, _: challenge::ActiveModel) -> Result<(), DbErr> {
        Err(down())
    }
    async fn take_challenge(&self, _: &str) -> Result<Option<challenge::Model>, DbErr> {
        Err(down())
    }
    async fn create_session(&self, _: session::ActiveModel) -> Result<(), DbErr> {
        Err(down())
    }
    async fn valid_session(&self, _: &str) -> Result<Option<session::Model>, DbErr> {
        Err(down())
    }
    async fn delete_session(&self, _: &str) -> Result<(), DbErr> {
        Err(down())
    }
    async fn delete_sessions_for(&self, _: &str) -> Result<u64, DbErr> {
        Err(down())
    }
    async fn sessions_for(&self, _: &str) -> Result<Vec<session::Model>, DbErr> {
        Err(down())
    }
    async fn purge_expired(&self, _: i64) -> Result<PurgeCounts, DbErr> {
        Err(down())
    }
    async fn insert_bundle(
        &self,
        _: bundle::ActiveModel,
        _: Vec<recipe_index::ActiveModel>,
        _: &Quota,
    ) -> Result<(), BundleInsertError> {
        Err(down().into())
    }
    async fn account_usage(&self, _: &str) -> Result<Usage, DbErr> {
        Err(down())
    }
    async fn unpublish(&self, _: &str, _: &str, _: Option<&str>) -> Result<Unpublish, DbErr> {
        Err(down())
    }
    async fn delete_account(&self, _: &str) -> Result<Option<Usage>, DbErr> {
        Err(down())
    }
    async fn bundle_latest(&self, _: &str) -> Result<Option<bundle::Model>, DbErr> {
        Err(down())
    }
    async fn bundle_exact(&self, _: &str, _: &str) -> Result<Option<bundle::Model>, DbErr> {
        Err(down())
    }
    async fn browse(&self, _: u64, _: u64) -> Result<Vec<recipe_index::Model>, DbErr> {
        Err(down())
    }
    async fn search(&self, _: &SearchQuery) -> Result<Vec<recipe_index::Model>, DbErr> {
        Err(down())
    }
}

#[async_trait]
impl Moderation for DownStorage {
    async fn file_report(&self, _: NewReport, _: u64) -> Result<FileReport, DbErr> {
        Err(down())
    }
    async fn reports(&self, _: Option<&str>, _: u64, _: u64) -> Result<Vec<report::Model>, DbErr> {
        Err(down())
    }
    async fn report(&self, _: i64) -> Result<Option<report::Model>, DbErr> {
        Err(down())
    }
    async fn resolve_report(&self, _: i64, _: &str, _: &str, _: i64) -> Result<Resolve, DbErr> {
        Err(down())
    }
    async fn take_down(&self, _: NewTakedown) -> Result<TakeDown, DbErr> {
        Err(down())
    }
    async fn tombstones(&self, _: &str) -> Result<Vec<takedown::Model>, DbErr> {
        Err(down())
    }
    async fn lift_takedown(&self, _: &str, _: &str, _: i64) -> Result<Lift, DbErr> {
        Err(down())
    }
    async fn suspension(&self, _: &str) -> Result<Option<suspension::Model>, DbErr> {
        Err(down())
    }
    async fn suspend(&self, _: NewSuspension) -> Result<Suspend, DbErr> {
        Err(down())
    }
    async fn unsuspend(
        &self,
        _: &str,
        _: &str,
        _: i64,
    ) -> Result<Option<moderation_action::Model>, DbErr> {
        Err(down())
    }
}

/// Asserts `response` is the outage answer: 503 problem+json carrying the
/// stable `unavailable` code and nothing about the underlying failure.
pub fn assert_unavailable(response: &TestResponse) {
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    assert_code(response, code::UNAVAILABLE);
    let body = response.text().to_lowercase();
    for leak in ["pool", "timed out", "connection", "sql"] {
        assert!(!body.contains(leak), "the 503 body leaks {leak:?}: {body}");
    }
}

/// Drives the probes and version endpoints with the database up and down,
/// and every database-backed operation with it down.
pub async fn probes_and_outages(coverage: &Coverage) {
    let up = Harness::covered(test_config(), Arc::new(MockStorage::default()), coverage);
    up.client
        .get("/api/v1/ready")
        .send()
        .await
        .assert_status(StatusCode::OK);
    session_lookup_failures(&up).await;

    let config = test_config();
    let app = App::new(
        Arc::new(DownStorage),
        Arc::new(platform()),
        Arc::new(config.clone()),
    );
    let service = crisp_server::router(&config.http)
        .build(app)
        .expect("router builds");
    let c = TestClient::new(service).with_coverage(coverage);

    // Liveness and version answers never depend on the database.
    for path in [
        "/api/v1/health",
        "/api/v1/server-info",
        "/api/v1/platform-keys",
    ] {
        c.get(path).send().await.assert_status(StatusCode::OK);
    }
    assert_unavailable(&c.get("/api/v1/ready").send().await);

    let author = SecretKeySet::from_seed(&[70u8; 32]);
    let profile = json!({ "profile": signed_profile(&author, "down", "Down") });
    assert_unavailable(&c.post("/api/v1/auth/register").json(&profile).send().await);
    let challenge = json!({ "username": "down" });
    assert_unavailable(
        &c.post("/api/v1/auth/challenge")
            .json(&challenge)
            .send()
            .await,
    );
    let login = json!({ "nonce": "00", "signature": "00" });
    assert_unavailable(&c.post("/api/v1/auth/login").json(&login).send().await);
    for path in [
        "/api/v1/accounts/usr1down",
        "/api/v1/bundles/doc1down",
        "/api/v1/bundles/doc1down/rev1down",
        "/api/v1/recipes",
        "/api/v1/search",
    ] {
        assert_unavailable(&c.get(path).send().await);
    }

    // A session the database cannot look up is a 503, not a 401: the client
    // must not conclude its token is invalid.
    let bearer = "Bearer some-session-token";
    let logout = c
        .post("/api/v1/auth/logout")
        .header("authorization", bearer);
    assert_unavailable(&logout.send().await);
    let logout_all = c
        .post("/api/v1/auth/logout-all")
        .header("authorization", bearer);
    assert_unavailable(&logout_all.send().await);
    let sessions = c
        .get("/api/v1/auth/sessions")
        .header("authorization", bearer);
    assert_unavailable(&sessions.send().await);
    let publish = c
        .post("/api/v1/bundles")
        .header("authorization", bearer)
        .body("application/octet-stream", vec![0x00]);
    assert_unavailable(&publish.send().await);
    let usage = c
        .get("/api/v1/account/usage")
        .header("authorization", bearer);
    assert_unavailable(&usage.send().await);
    for path in [
        "/api/v1/account",
        "/api/v1/bundles/doc1down",
        "/api/v1/bundles/doc1down/rev1down",
        "/api/v1/admin/takedowns/doc1down",
    ] {
        assert_unavailable(&c.delete(path).header("authorization", bearer).send().await);
    }
    let reports = c
        .get("/api/v1/admin/reports")
        .header("authorization", bearer);
    assert_unavailable(&reports.send().await);
    let report = json!({ "doc_id": "doc1down", "category": "spam" });
    let takedown = json!({ "doc_id": "doc1down", "reason": "spam" });
    for (path, body) in [
        ("/api/v1/reports", report),
        (
            "/api/v1/admin/reports/1/resolve",
            json!({ "resolution": "dismissed" }),
        ),
        ("/api/v1/admin/takedowns", takedown),
        (
            "/api/v1/admin/accounts/usr1down/suspend",
            json!({ "reason": "spam" }),
        ),
    ] {
        let post = c.post(path).header("authorization", bearer).json(&body);
        assert_unavailable(&post.send().await);
    }
    let unsuspend = c
        .post("/api/v1/admin/accounts/usr1down/unsuspend")
        .header("authorization", bearer);
    assert_unavailable(&unsuspend.send().await);
    // Without a token the answer is still 401: nothing was looked up.
    c.post("/api/v1/auth/logout")
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    c.assert_conformance();
}

/// A bearer token whose session or account lookup fails with a database
/// error that is not an outage answers 500 on logout and publish: neither a
/// 401 (the token was never judged invalid) nor a 503.
async fn session_lookup_failures(h: &Harness) {
    let author = SecretKeySet::from_seed(&[71u8; 32]);
    let token = h.register_and_login(&author, "faulty").await;
    for op in ["valid_session", "account_by_usr_id"] {
        h.storage.inject(op, Fault::Db);
        let logout = h
            .client
            .post("/api/v1/auth/logout")
            .header("authorization", &format!("Bearer {token}"));
        let response = logout.send().await;
        response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
        assert_code(&response, code::INTERNAL);
        let response = h.publish(&token, vec![0x00]).await;
        response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
        assert_code(&response, code::INTERNAL);
        h.storage.heal();
    }
}
