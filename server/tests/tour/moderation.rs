//! The responses the moderation routes declare — `POST /reports` and every
//! `/admin` route — and those moderation adds to retrieval, publishing,
//! registration, and login; plus the helpers that send them.

use std::sync::Arc;

use crisp_core::sign::SecretKeySet;
use crisp_server::config::Config;
use crisp_server::error::code;
use crisp_server::moderation::reports::MAX_OPEN_REPORTS;
use kynos::http::StatusCode;
use kynos::test::{Coverage, TestResponse};
use serde_json::{Value, json};

use crate::common::{
    Fault, Harness, MockStorage, RecipeSpec, assert_code, build_recipe_bundle_with, signed_profile,
    test_config,
};

/// The key of the administrator the moderation tests configure.
pub fn admin_key() -> SecretKeySet {
    SecretKeySet::from_seed(&[120u8; 32])
}

/// A test configuration listing [`admin_key`]'s account as administrator.
pub fn admin_config() -> Config {
    let mut config = test_config();
    let admin = admin_key().user_id().expect("admin id").0;
    config.moderation.admins.insert(admin);
    config
}

/// The `authorization` header value for `token`.
fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// Sends an authenticated `POST {path}` (under `/api/v1`) with a JSON body.
pub async fn post(h: &Harness, token: &str, path: &str, body: &Value) -> TestResponse {
    h.client
        .post(&format!("/api/v1{path}"))
        .header("authorization", &bearer(token))
        .json(body)
        .send()
        .await
}

/// Sends an authenticated `POST {path}` with no body.
pub async fn post_empty(h: &Harness, token: &str, path: &str) -> TestResponse {
    h.client
        .post(&format!("/api/v1{path}"))
        .header("authorization", &bearer(token))
        .send()
        .await
}

/// Sends an authenticated `GET {path}`.
pub async fn get(h: &Harness, token: &str, path: &str) -> TestResponse {
    h.client
        .get(&format!("/api/v1{path}"))
        .header("authorization", &bearer(token))
        .send()
        .await
}

/// Sends an authenticated `DELETE {path}`.
pub async fn delete(h: &Harness, token: &str, path: &str) -> TestResponse {
    h.client
        .delete(&format!("/api/v1{path}"))
        .header("authorization", &bearer(token))
        .send()
        .await
}

/// Publishes a fresh revision (`modified_at`) by `author`, asserting 201;
/// returns `(doc_id, revision)`.
pub async fn published(
    h: &Harness,
    author: &SecretKeySet,
    token: &str,
    modified_at: u64,
) -> (String, String) {
    let spec = RecipeSpec {
        modified_at,
        ..RecipeSpec::default()
    };
    let (bytes, doc_id, revision) = build_recipe_bundle_with(author, &spec);
    h.publish(token, bytes)
        .await
        .assert_status(StatusCode::CREATED);
    (doc_id, revision)
}

/// Sends the four JSON body rejections kynos declares, authenticated with
/// `token`: 400 (syntax), 413 (over 2 MiB), 415 (media type), 422 (schema).
async fn json_rejections(h: &Harness, token: &str, path: &str) {
    let path = format!("/api/v1{path}");
    let send = |content_type: &'static str, body: String| {
        h.client
            .post(&path)
            .header("authorization", &bearer(token))
            .body(content_type, body)
            .send()
    };
    let rejected = [
        (
            send("application/json", "{not json".into()).await,
            StatusCode::BAD_REQUEST,
        ),
        (
            send(
                "application/json",
                format!("{{\"pad\":\"{}\"}}", "x".repeat(3 << 20)),
            )
            .await,
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
        (
            send("text/plain", "hello".into()).await,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            send("application/json", "{\"unexpected\":1}".into()).await,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ];
    for (response, status) in rejected {
        response.assert_status(status);
    }
}

/// Asserts `response` is a 500 problem.
fn assert_internal(response: &TestResponse) {
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    assert_code(response, code::INTERNAL);
}

/// Drives `POST /reports` and every `/admin` route through each response
/// it declares but 503 (in [`super::outage`]), and the moderation answers
/// of retrieval, publishing, registration, and login.
pub async fn moderation_responses(coverage: &Coverage) {
    let storage = Arc::new(MockStorage::default());
    let h = Harness::covered(admin_config(), Arc::clone(&storage), coverage);
    let publisher = SecretKeySet::from_seed(&[121u8; 32]);
    let admin = h.register_and_login(&admin_key(), "tour-admin").await;
    let user = h.register_and_login(&publisher, "tour-publisher").await;
    let (doc_id, revision) = published(&h, &publisher, &user, 2_000).await;

    reports(&h, &storage, &admin, &user, &doc_id).await;
    takedowns(&h, &storage, &admin, &user, &doc_id, &revision, &publisher).await;
    suspension(&h, &storage, &admin, &user, &publisher).await;
    h.client.assert_conformance();
}

/// `POST /reports`, `GET /admin/reports`, `POST /admin/reports/{id}/resolve`.
async fn reports(h: &Harness, storage: &MockStorage, admin: &str, user: &str, doc_id: &str) {
    let report = json!({ "doc_id": doc_id, "category": "spam", "detail": "buy now" });
    let filed = post(h, user, "/reports", &report).await;
    filed.assert_status(StatusCode::OK);
    let id = filed.json::<Value>()["id"].as_i64().expect("a report id");
    let long = json!({ "doc_id": doc_id, "category": "spam", "detail": "x".repeat(2001) });
    post(h, user, "/reports", &long)
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    let missing = json!({ "doc_id": "rcp1none", "category": "other" });
    let response = post(h, user, "/reports", &missing).await;
    response.assert_status(StatusCode::NOT_FOUND);
    assert_code(&response, code::NOT_FOUND);
    for _ in 0..MAX_OPEN_REPORTS {
        post(h, admin, "/reports", &report)
            .await
            .assert_status(StatusCode::OK);
    }
    let response = post(h, admin, "/reports", &report).await;
    response.assert_status(StatusCode::CONFLICT);
    assert_code(&response, code::CONFLICT);
    post(h, "not-a-token", "/reports", &report)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    json_rejections(h, user, "/reports").await;
    storage.inject("file_report", Fault::Db);
    assert_internal(&post(h, user, "/reports", &report).await);
    storage.heal();

    let list = get(h, admin, "/admin/reports?status=open&limit=5").await;
    list.assert_status(StatusCode::OK);
    assert_eq!(list.json::<Value>()["reports"].as_array().unwrap().len(), 5);
    get(h, user, "/admin/reports")
        .await
        .assert_status(StatusCode::FORBIDDEN);
    get(h, "not-a-token", "/admin/reports")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    get(h, admin, "/admin/reports?status=closed")
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    storage.inject("reports", Fault::Db);
    assert_internal(&get(h, admin, "/admin/reports").await);
    storage.heal();

    let resolve = format!("/admin/reports/{id}/resolve");
    let dismissed = json!({ "resolution": "dismissed" });
    storage.inject("resolve_report", Fault::Db);
    assert_internal(&post(h, admin, &resolve, &dismissed).await);
    storage.heal();
    post(h, user, &resolve, &dismissed)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    post(h, "not-a-token", &resolve, &dismissed)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    post(h, admin, &resolve, &dismissed)
        .await
        .assert_status(StatusCode::OK);
    post(h, admin, &resolve, &dismissed)
        .await
        .assert_status(StatusCode::CONFLICT);
    let response = post(h, admin, "/admin/reports/999999/resolve", &dismissed).await;
    response.assert_status(StatusCode::NOT_FOUND);
    post(h, admin, "/admin/reports/abc/resolve", &dismissed)
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    json_rejections(h, admin, &resolve).await;
}

/// `POST /admin/takedowns`, `DELETE /admin/takedowns/{doc_id}`, and the
/// 410 and 409 `removed` answers they cause.
async fn takedowns(
    h: &Harness,
    storage: &MockStorage,
    admin: &str,
    user: &str,
    doc_id: &str,
    revision: &str,
    publisher: &SecretKeySet,
) {
    let takedown = json!({ "doc_id": doc_id, "revision": revision, "reason": "copyright" });
    storage.inject("take_down", Fault::Db);
    assert_internal(&post(h, admin, "/admin/takedowns", &takedown).await);
    storage.heal();
    post(h, user, "/admin/takedowns", &takedown)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    post(h, "not-a-token", "/admin/takedowns", &takedown)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    let unknown_report = json!({ "doc_id": doc_id, "reason": "spam", "report_id": 999999 });
    post(h, admin, "/admin/takedowns", &unknown_report)
        .await
        .assert_status(StatusCode::NOT_FOUND);
    post(h, admin, "/admin/takedowns", &takedown)
        .await
        .assert_status(StatusCode::OK);
    post(h, admin, "/admin/takedowns", &takedown)
        .await
        .assert_status(StatusCode::CONFLICT);
    let missing = json!({ "doc_id": "rcp1none", "reason": "spam" });
    post(h, admin, "/admin/takedowns", &missing)
        .await
        .assert_status(StatusCode::NOT_FOUND);
    json_rejections(h, admin, "/admin/takedowns").await;

    for path in [
        format!("/bundles/{doc_id}"),
        format!("/bundles/{doc_id}/{revision}"),
    ] {
        let response = get(h, user, &path).await;
        response.assert_status(StatusCode::GONE);
        assert_code(&response, code::REMOVED);
    }
    let (bytes, _, _) = build_recipe_bundle_with(publisher, &RecipeSpec::default());
    let response = h.publish(user, bytes).await;
    response.assert_status(StatusCode::CONFLICT);
    assert_code(&response, code::REMOVED);

    let lift = format!("/admin/takedowns/{doc_id}");
    storage.inject("lift_takedown", Fault::Db);
    assert_internal(&delete(h, admin, &lift).await);
    storage.heal();
    delete(h, user, &lift)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    delete(h, "not-a-token", &lift)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    delete(h, admin, &format!("{lift}%FF"))
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    delete(h, admin, &lift)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    delete(h, admin, &lift)
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

/// `POST /admin/accounts/{usr_id}/suspend` and `/unsuspend`, and the 403
/// `account_suspended` answers of login and registration.
async fn suspension(
    h: &Harness,
    storage: &MockStorage,
    admin: &str,
    user: &str,
    publisher: &SecretKeySet,
) {
    let usr_id = publisher.user_id().unwrap().0;
    let suspend = format!("/admin/accounts/{usr_id}/suspend");
    let unsuspend = format!("/admin/accounts/{usr_id}/unsuspend");
    let body = json!({ "reason": "spam", "take_down_all": true });
    storage.inject("suspend", Fault::Db);
    assert_internal(&post(h, admin, &suspend, &body).await);
    storage.heal();
    post(h, user, &suspend, &body)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    post(h, "not-a-token", &suspend, &body)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    let unknown = post(h, admin, "/admin/accounts/usr1none/suspend", &body).await;
    unknown.assert_status(StatusCode::NOT_FOUND);
    post(h, admin, "/admin/accounts/%FF/suspend", &body)
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    json_rejections(h, admin, &suspend).await;
    post(h, admin, &suspend, &body)
        .await
        .assert_status(StatusCode::OK);
    post(h, admin, &suspend, &body)
        .await
        .assert_status(StatusCode::CONFLICT);

    let (nonce, audience) = h.challenge("tour-publisher").await;
    let response = h.login(publisher, &nonce, &audience).await;
    response.assert_status(StatusCode::FORBIDDEN);
    assert_code(&response, code::ACCOUNT_SUSPENDED);
    let profile = json!({ "profile": signed_profile(publisher, "tour-again", "Again") });
    let response = h
        .client
        .post("/api/v1/auth/register")
        .json(&profile)
        .send()
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
    assert_code(&response, code::ACCOUNT_SUSPENDED);

    storage.inject("unsuspend", Fault::Db);
    assert_internal(&post_empty(h, admin, &unsuspend).await);
    storage.heal();
    post_empty(h, user, &unsuspend)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    post_empty(h, "not-a-token", &unsuspend)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    let other = SecretKeySet::from_seed(&[122u8; 32]);
    let bystander = h.register_and_login(&other, "tour-bystander").await;
    post_empty(h, &bystander, &unsuspend)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    post_empty(h, admin, "/admin/accounts/%FF/unsuspend")
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    post_empty(h, admin, &unsuspend)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    post_empty(h, admin, &unsuspend)
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
