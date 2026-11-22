//! The probes and the 503 every database-backed operation declares: a
//! server whose database is down keeps answering liveness and version
//! requests, fails readiness, and answers every operation that needs the
//! database with an opaque 503 problem - never 401, never 500.

use std::sync::Arc;

use async_trait::async_trait;
use crisp_core::sign::SecretKeySet;
use crisp_server::db::{
    account, bundle, challenge, moderation_action, recipe_index, report, revocation, session,
    suspension, takedown,
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
    async fn revocations(&self) -> Result<Vec<revocation::Model>, DbErr> {
        Err(down())
    }
    async fn is_revoked(&self, _: &str) -> Result<bool, DbErr> {
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
        "/api/v1/revocations",
    ] {
        assert_unavailable(&c.get(path).send().await);
    }

    // A session the database cannot look up is a 503, not a 401: the client
    // must not conclude its token is invalid.
    let token = "some-session-token";
    let logout = c.post("/api/v1/auth/logout").bearer(token);
    assert_unavailable(&logout.send().await);
    let logout_all = c.post("/api/v1/auth/logout-all").bearer(token);
    assert_unavailable(&logout_all.send().await);
    let sessions = c.get("/api/v1/auth/sessions").bearer(token);
    assert_unavailable(&sessions.send().await);
    let publish = c
        .post("/api/v1/bundles")
        .bearer(token)
        .body("application/octet-stream", vec![0x00]);
    assert_unavailable(&publish.send().await);
    let usage = c.get("/api/v1/account/usage").bearer(token);
    assert_unavailable(&usage.send().await);
    for path in [
        "/api/v1/account",
        "/api/v1/bundles/doc1down",
        "/api/v1/bundles/doc1down/rev1down",
        "/api/v1/admin/takedowns/doc1down",
    ] {
        assert_unavailable(&c.delete(path).bearer(token).send().await);
    }
    let reports = c.get("/api/v1/admin/reports").bearer(token);
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
        let post = c.post(path).bearer(token).json(&body);
        assert_unavailable(&post.send().await);
    }
    let unsuspend = c
        .post("/api/v1/admin/accounts/usr1down/unsuspend")
        .bearer(token);
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
        let logout = h.client.post("/api/v1/auth/logout").bearer(&token);
        let response = logout.send().await;
        response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
        assert_code(&response, code::INTERNAL);
        let response = h.publish(&token, vec![0x00]).await;
        response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
        assert_code(&response, code::INTERNAL);
        h.storage.heal();
    }
    admin_lookup_failures(h, &token).await;
}

/// The moderation state an `/admin` handler could change.
fn moderation_state(
    h: &Harness,
) -> (
    Vec<report::Model>,
    Vec<takedown::Model>,
    Vec<suspension::Model>,
    Vec<moderation_action::Model>,
) {
    let inner = h.storage.inner.lock().expect("storage lock");
    (
        inner.reports.clone(),
        inner.takedowns.clone(),
        inner.suspensions.clone(),
        inner.actions.clone(),
    )
}

/// A session lookup the database interrupted reaches the admin authorizer
/// unjudged, and it lets the request through so the handler can answer the
/// failure (`SessionLookup` in `src/auth/bearer.rs`). Every `/admin` route is
/// safe only because its handler resolves the lookup before anything else:
/// here a *non-administrator's* token, with every other storage call
/// healthy, must still be answered 500 on each one, and leave the moderation
/// state untouched. The `valid_session` pass is the one that proves it for
/// every route: under the `account_by_usr_id` fault, suspend and unsuspend
/// could fail on a lookup of their own. The requests come from
/// [`ADMIN_CALLS`], which is checked against every operation the API
/// publishes as demanding the `admin` scope, so a new one fails here until
/// it is driven too.
async fn admin_lookup_failures(h: &Harness, token: &str) {
    h.storage
        .inner
        .lock()
        .expect("storage lock")
        .reports
        .push(report::Model {
            id: 1,
            reporter: None,
            doc_id: "doc1guard".to_owned(),
            revision: None,
            category: "spam".to_owned(),
            detail: String::new(),
            status: "open".to_owned(),
            created_at: 0,
            resolved_at: None,
            resolved_by: None,
        });
    let before = moderation_state(h);
    let c = &h.client;
    for op in ["valid_session", "account_by_usr_id"] {
        h.storage.inject(op, Fault::Db);
        for call in ADMIN_CALLS {
            let request = match call.method {
                "get" => c.get(call.path),
                "post" => c.post(call.path),
                "delete" => c.delete(call.path),
                other => panic!("add a `{other}` arm to drive {}", call.template),
            };
            let request = match &call.body {
                Some(body) => request
                    .json(&serde_json::from_str::<serde_json::Value>(body).expect("a JSON body")),
                None => request,
            };
            let response = request.bearer(token).send().await;
            response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
            assert_code(&response, code::INTERNAL);
        }
        h.storage.heal();
    }
    assert!(
        moderation_state(h) == before,
        "an /admin handler acted on a session lookup that failed"
    );
    let mut driven: Vec<(String, String)> = ADMIN_CALLS
        .iter()
        .map(|call| (call.method.to_owned(), call.template.to_owned()))
        .collect();
    driven.sort();
    assert_eq!(
        admin_operations(),
        driven,
        "every operation demanding the admin scope must be in ADMIN_CALLS"
    );
}

/// One request [`admin_lookup_failures`] sends to an admin-scoped operation.
struct AdminCall {
    /// The operation's method, as the OpenAPI document keys it.
    method: &'static str,
    /// The operation's path template, as the OpenAPI document keys it.
    template: &'static str,
    /// The concrete path sent.
    path: &'static str,
    /// The JSON body sent, if any.
    body: Option<&'static str>,
}

/// Every request [`admin_lookup_failures`] sends: the one list both the
/// requests and the coverage check are read from.
const ADMIN_CALLS: &[AdminCall] = &[
    AdminCall {
        method: "get",
        template: "/api/v1/admin/reports",
        path: "/api/v1/admin/reports",
        body: None,
    },
    AdminCall {
        method: "post",
        template: "/api/v1/admin/reports/{id}/resolve",
        path: "/api/v1/admin/reports/1/resolve",
        body: Some(r#"{"resolution":"actioned"}"#),
    },
    AdminCall {
        method: "post",
        template: "/api/v1/admin/takedowns",
        path: "/api/v1/admin/takedowns",
        body: Some(r#"{"doc_id":"doc1guard","reason":"spam"}"#),
    },
    AdminCall {
        method: "delete",
        template: "/api/v1/admin/takedowns/{doc_id}",
        path: "/api/v1/admin/takedowns/doc1guard",
        body: None,
    },
    AdminCall {
        method: "post",
        template: "/api/v1/admin/accounts/{usr_id}/suspend",
        path: "/api/v1/admin/accounts/usr1guard/suspend",
        body: Some(r#"{"reason":"spam"}"#),
    },
    AdminCall {
        method: "post",
        template: "/api/v1/admin/accounts/{usr_id}/unsuspend",
        path: "/api/v1/admin/accounts/usr1guard/unsuspend",
        body: None,
    },
];

/// Every operation in the committed OpenAPI document (which
/// `tests/openapi.rs` holds equal to the router's) that demands the `admin`
/// scope of `sessionToken`, whatever its path or method, as
/// `(method, path)`, sorted.
fn admin_operations() -> Vec<(String, String)> {
    /// The Path Item fields that are not operations.
    const NOT_OPERATIONS: &[&str] = &["parameters", "summary", "description", "servers", "$ref"];
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../spec/openapi.json");
    let document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("reads the document"))
            .expect("the document is JSON");
    let demands_admin = |operation: &serde_json::Value| {
        operation["security"]
            .as_array()
            .is_some_and(|requirements| {
                requirements.iter().any(|requirement| {
                    requirement["sessionToken"]
                        .as_array()
                        .is_some_and(|scopes| scopes.iter().any(|scope| scope == "admin"))
                })
            })
    };
    let mut operations: Vec<(String, String)> = document["paths"]
        .as_object()
        .expect("the document has paths")
        .iter()
        .flat_map(|(path, item)| {
            item.as_object()
                .expect("a path item is an object")
                .iter()
                .filter(|(key, operation)| {
                    !NOT_OPERATIONS.contains(&key.as_str()) && demands_admin(operation)
                })
                .map(|(method, _)| (method.clone(), path.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    operations.sort();
    operations
}
