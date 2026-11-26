//! The responses the ingredient-suggestion routes declare —
//! `POST /ingredients/suggestions` and the `/admin/ingredients/suggestions`
//! queue, triage, and export — but 429 (in [`super::throttled_responses`])
//! and 503 (in [`super::outage`]).

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use crisp_core::sign::SecretKeySet;
use crisp_server::db::ingredient_suggestion;
use crisp_server::error::code;
use crisp_server::ingredients::suggestions::MAX_OPEN_SUGGESTIONS;
use crisp_server::ingredients::triage::MAX_EXPORT;
use crisp_server::limits::Budget;
use kynos::http::StatusCode;
use kynos::test::{Coverage, TestResponse};
use serde_json::{Value, json};

use super::moderation::{admin_config, admin_key, get, json_rejections, post};
use crate::common::{Fault, Harness, MockStorage, assert_code};

/// Asserts `response` is a 500 problem.
fn assert_internal(response: &TestResponse) {
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    assert_code(response, code::INTERNAL);
}

/// Drives every suggestion route through each response it declares but 429
/// and 503.
pub async fn suggestion_responses(coverage: &Coverage) {
    let storage = Arc::new(MockStorage::default());
    let mut config = admin_config();
    // Room to fill a suggester's open-suggestion cap within one hour.
    config.http.limits.suggest = Budget::new(NonZeroU32::MAX, Duration::from_secs(3600));
    let h = Harness::covered(config, Arc::clone(&storage), coverage);
    let admin = h.register_and_login(&admin_key(), "tour-curator").await;
    let user = h
        .register_and_login(&SecretKeySet::from_seed(&[122u8; 32]), "tour-suggester")
        .await;

    let id = file(&h, &storage, &admin, &user).await;
    list(&h, &storage, &admin, &user).await;
    resolve(&h, &storage, &admin, &user, id).await;
    export(&h, &storage, &admin, &user).await;
    h.client.assert_conformance();
}

/// `POST /ingredients/suggestions`; returns the id of the first suggestion
/// filed.
async fn file(h: &Harness, storage: &MockStorage, admin: &str, user: &str) -> i64 {
    let path = "/ingredients/suggestions";
    let alias = json!({
        "kind": "name", "slug": "kimchi-aged", "locale": "en-US", "text": "mugunji",
        "note": "what the shop calls it",
    });
    let filed = post(h, user, path, &alias).await;
    filed.assert_status(StatusCode::OK);
    let id = filed.json::<Value>()["id"]
        .as_i64()
        .expect("a suggestion id");
    let unknown =
        json!({ "kind": "name", "slug": "no-such-entry", "locale": "en-US", "text": "x" });
    let response = post(h, user, path, &unknown).await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert_code(&response, code::BAD_REQUEST);
    let known =
        json!({ "kind": "name", "slug": "kimchi-aged", "locale": "en-US", "text": "aged kimchi" });
    let response = post(h, user, path, &known).await;
    response.assert_status(StatusCode::CONFLICT);
    assert_code(&response, code::CONFLICT);
    let other = json!({ "kind": "other", "locale": "en-US", "text": "split the soy sauces" });
    for _ in 0..MAX_OPEN_SUGGESTIONS {
        post(h, admin, path, &other)
            .await
            .assert_status(StatusCode::OK);
    }
    let response = post(h, admin, path, &other).await;
    response.assert_status(StatusCode::CONFLICT);
    post(h, "not-a-token", path, &other)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    json_rejections(h, user, path).await;
    storage.inject("file_suggestion", Fault::Db);
    assert_internal(&post(h, user, path, &other).await);
    storage.heal();
    id
}

/// `GET /admin/ingredients/suggestions`.
async fn list(h: &Harness, storage: &MockStorage, admin: &str, user: &str) {
    let path = "/admin/ingredients/suggestions";
    let open = get(h, admin, &format!("{path}?status=open&limit=5")).await;
    open.assert_status(StatusCode::OK);
    let suggestions = open.json::<Value>()["suggestions"].clone();
    assert_eq!(suggestions.as_array().unwrap().len(), 5);
    assert_eq!(suggestions[0]["text"], "mugunji");
    get(h, user, path)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    get(h, "not-a-token", path)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    get(h, admin, &format!("{path}?status=closed"))
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    storage.inject("suggestions", Fault::Db);
    assert_internal(&get(h, admin, path).await);
    storage.heal();
}

/// `POST /admin/ingredients/suggestions/{id}/resolve`, which writes an
/// audit row.
async fn resolve(h: &Harness, storage: &MockStorage, admin: &str, user: &str, id: i64) {
    let path = format!("/admin/ingredients/suggestions/{id}/resolve");
    let accepted = json!({ "resolution": "accepted" });
    storage.inject("resolve_suggestion", Fault::Db);
    assert_internal(&post(h, admin, &path, &accepted).await);
    storage.heal();
    post(h, user, &path, &accepted)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    post(h, "not-a-token", &path, &accepted)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    let resolved = post(h, admin, &path, &accepted).await;
    resolved.assert_status(StatusCode::OK);
    assert_eq!(resolved.json::<Value>()["status"], "accepted");
    let action = storage.inner.lock().unwrap().actions.last().cloned();
    let action = action.expect("an audit row");
    assert_eq!(
        (
            action.action.as_str(),
            action.target,
            action.reason.as_deref()
        ),
        (
            "resolve_suggestion",
            format!("suggestion/{id}"),
            Some("accepted")
        )
    );
    let response = post(h, admin, &path, &accepted).await;
    response.assert_status(StatusCode::CONFLICT);
    assert_code(&response, code::CONFLICT);
    let missing = "/admin/ingredients/suggestions/999999/resolve";
    post(h, admin, missing, &accepted)
        .await
        .assert_status(StatusCode::NOT_FOUND);
    post(
        h,
        admin,
        "/admin/ingredients/suggestions/abc/resolve",
        &accepted,
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
    json_rejections(h, admin, &path).await;
}

/// `GET /admin/ingredients/suggestions/export`, after the suggestion
/// [`resolve`] accepted.
async fn export(h: &Harness, storage: &MockStorage, admin: &str, user: &str) {
    let path = "/admin/ingredients/suggestions/export";
    let exported = get(h, admin, path).await;
    exported.assert_status(StatusCode::OK);
    let body = exported.json::<Value>();
    assert_eq!(body["complete"], true);
    // `names/en-US.json` already holds a `kimchi-aged` line, so the export
    // replaces it with a merged one rather than appending a duplicate key.
    let patch = &body["files"][0];
    assert_eq!(body["files"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        patch["path"],
        "tools/ingredients/rules/overrides/names/en-US.json"
    );
    assert_eq!(patch["lines"], json!([]));
    let replaced = patch["replace"][0].as_str().expect("a replacement line");
    assert!(replaced.starts_with(r#""kimchi-aged": {"name": "aged kimchi""#));
    assert!(replaced.contains(r#""sour kimchi""#) && replaced.ends_with(r#""mugunji"]}"#));
    let later = get(h, admin, &format!("{path}?since=99999999999")).await;
    later.assert_status(StatusCode::OK);
    assert_eq!(later.json::<Value>()["suggestions"], json!([]));
    get(h, user, path)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    get(h, "not-a-token", path)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    get(h, admin, &format!("{path}?since=soon"))
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    get(h, admin, &format!("{path}?after=1"))
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    storage.inject("accepted_suggestions", Fault::Db);
    assert_internal(&get(h, admin, path).await);
    storage.heal();
    continuation(h, storage, admin).await;
}

/// An export past [`MAX_EXPORT`] rows is incomplete, and continuing it from
/// its `last_accepted_at` and `last_id` repeats none of the suggestions
/// accepted in that boundary second.
async fn continuation(h: &Harness, storage: &MockStorage, admin: &str) {
    let path = "/admin/ingredients/suggestions/export";
    let ids = |body: &Value| -> Vec<i64> {
        let ids = body["suggestions"].as_array().expect("suggestion ids");
        ids.iter().map(|id| id.as_i64().expect("an id")).collect()
    };
    let before = ids(&get(h, admin, path).await.json::<Value>());
    // MAX_EXPORT + 1 suggestions all accepted in the same second, before
    // the one `resolve` accepted.
    let seeded: Vec<i64> = (0..=MAX_EXPORT as i64).map(|n| 1_000_000 + n).collect();
    let rows = seeded.iter().map(|&id| ingredient_suggestion::Model {
        id,
        suggester: None,
        kind: "other".to_owned(),
        slug: None,
        locale: "en-US".to_owned(),
        text: format!("bulk {id}"),
        note: String::new(),
        status: "accepted".to_owned(),
        created_at: 1,
        resolved_at: Some(1),
        resolved_by: None,
    });
    storage.inner.lock().unwrap().suggestions.extend(rows);

    let first = get(h, admin, path).await.json::<Value>();
    assert_eq!(first["complete"], false);
    assert_eq!(ids(&first), seeded[..MAX_EXPORT as usize]);
    assert_eq!(first["last_accepted_at"], 1);
    assert_eq!(first["last_id"], seeded[MAX_EXPORT as usize - 1]);

    let next = format!(
        "{path}?since={}&after={}",
        first["last_accepted_at"], first["last_id"]
    );
    let rest = get(h, admin, &next).await.json::<Value>();
    assert_eq!(rest["complete"], true);
    let mut expected = vec![seeded[MAX_EXPORT as usize]];
    expected.extend(before);
    assert_eq!(ids(&rest), expected);
}
