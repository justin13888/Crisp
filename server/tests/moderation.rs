//! Moderation over the real router: reports, takedowns and their
//! tombstones, suspension, the audit trail every administrator action
//! writes, and the abuse contact and terms `GET /server-info` advertises.

mod common;
mod tour;

use common::{Harness, assert_code, signed_profile};
use crisp_core::sign::SecretKeySet;
use crisp_server::db::{bundle, suspension};
use crisp_server::error::code;
use crisp_server::storage::Storage;
use kynos::http::StatusCode;
use serde_json::{Value, json};
use tour::moderation::{admin_config, admin_key, delete, get, post, post_empty, published};

/// A harness with [`admin_key`] as administrator, signed in; returns it
/// with the administrator's token.
async fn moderated() -> (Harness, String) {
    let h = Harness::with_config(admin_config());
    let admin = h.register_and_login(&admin_key(), "admin").await;
    (h, admin)
}

/// Stores a bundle row as if `published_by` had published it.
fn put_bundle(h: &Harness, doc_id: &str, revision: &str, published_by: &str, published_at: i64) {
    h.storage.inner.lock().unwrap().bundles.push(bundle::Model {
        content_hash: format!("{doc_id}-{revision}"),
        doc_id: doc_id.to_owned(),
        revision: revision.to_owned(),
        kind: "recipe".to_owned(),
        format_version: 1,
        raw: revision.as_bytes().to_vec(),
        size_bytes: 10,
        published_by: published_by.to_owned(),
        published_at,
        endorsed_at: published_at,
    });
}

/// The `(action, target, reason)` of every audit row, in order.
fn audit_trail(h: &Harness) -> Vec<(String, String, Option<String>)> {
    (h.storage.inner.lock().unwrap().actions.iter())
        .map(|a| (a.action.clone(), a.target.clone(), a.reason.clone()))
        .collect()
}

fn entry(action: &str, target: &str, reason: Option<&str>) -> (String, String, Option<String>) {
    (
        action.to_owned(),
        target.to_owned(),
        reason.map(str::to_owned),
    )
}

/// Signs `author` in as `username`, returning the raw login response.
async fn sign_in(h: &Harness, author: &SecretKeySet, username: &str) -> kynos::test::TestResponse {
    let (nonce, audience) = h.challenge(username).await;
    h.login(author, &nonce, &audience).await
}

#[tokio::test]
async fn every_admin_route_refuses_a_non_administrator() {
    let (h, _) = moderated().await;
    let user = h
        .register_and_login(&SecretKeySet::from_seed(&[130u8; 32]), "user")
        .await;
    let body = json!({ "doc_id": "rcp1x", "reason": "spam", "resolution": "dismissed" });
    for path in [
        "/admin/reports/1/resolve",
        "/admin/takedowns",
        "/admin/accounts/usr1x/suspend",
    ] {
        post(&h, &user, path, &body)
            .await
            .assert_status(StatusCode::FORBIDDEN);
    }
    let unsuspend = post_empty(&h, &user, "/admin/accounts/usr1x/unsuspend").await;
    unsuspend.assert_status(StatusCode::FORBIDDEN);
    get(&h, &user, "/admin/reports")
        .await
        .assert_status(StatusCode::FORBIDDEN);
    delete(&h, &user, "/admin/takedowns/rcp1x")
        .await
        .assert_status(StatusCode::FORBIDDEN);
    assert!(audit_trail(&h).is_empty());
    h.client.assert_conformance();
}

#[tokio::test]
async fn with_no_administrators_configured_no_account_administers() {
    let h = Harness::new();
    let token = h.register_and_login(&admin_key(), "admin").await;
    get(&h, &token, "/admin/reports")
        .await
        .assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_takedown_removes_every_publishers_revisions_and_leaves_a_tombstone() {
    let (h, admin) = moderated().await;
    put_bundle(&h, "rcp1doc", "rev1a", "usr1alice", 10);
    put_bundle(&h, "rcp1doc", "rev1b", "usr1bob", 20);
    put_bundle(&h, "rcp1other", "rev1c", "usr1alice", 30);

    let takedown = json!({ "doc_id": "rcp1doc", "reason": "copyright" });
    let response = post(&h, &admin, "/admin/takedowns", &takedown).await;
    response.assert_status(StatusCode::OK);
    let tombstone: Value = response.json();
    assert_eq!(tombstone["revision"], Value::Null);
    assert_eq!(tombstone["reason"], "copyright");
    let mut hashes: Vec<String> =
        serde_json::from_value(tombstone["content_hashes"].clone()).unwrap();
    hashes.sort();
    assert_eq!(hashes, ["rcp1doc-rev1a", "rcp1doc-rev1b"]);
    let left: Vec<String> = (h.storage.inner.lock().unwrap().bundles.iter())
        .map(|b| b.doc_id.clone())
        .collect();
    assert_eq!(left, ["rcp1other"]);

    for path in ["/api/v1/bundles/rcp1doc", "/api/v1/bundles/rcp1doc/rev1a"] {
        let response = h.client.get(path).send().await;
        response.assert_status(StatusCode::GONE);
        assert_code(&response, code::REMOVED);
        assert_eq!(response.json::<Value>()["reason"], "copyright");
    }
    // A revision that never existed of a removed document is removed too.
    h.client
        .get("/api/v1/bundles/rcp1doc/rev1never")
        .send()
        .await
        .assert_status(StatusCode::GONE);
    h.client
        .get("/api/v1/bundles/rcp1none")
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
    assert_eq!(
        audit_trail(&h),
        [entry("takedown", "rcp1doc", Some("copyright"))]
    );

    // Lifting deletes the tombstone; the bundles are not restored.
    delete(&h, &admin, "/admin/takedowns/rcp1doc")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    h.client
        .get("/api/v1/bundles/rcp1doc")
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
    assert_eq!(audit_trail(&h)[1], entry("lift_takedown", "rcp1doc", None));
    h.client.assert_conformance();
}

#[tokio::test]
async fn a_taken_down_document_cannot_be_published_until_the_takedown_is_lifted() {
    let (h, admin) = moderated().await;
    let author = SecretKeySet::from_seed(&[131u8; 32]);
    let token = h.register_and_login(&author, "author").await;
    let (doc_id, _) = published(&h, &author, &token, 2_000).await;
    let takedown = json!({ "doc_id": doc_id, "reason": "illegal" });
    post(&h, &admin, "/admin/takedowns", &takedown)
        .await
        .assert_status(StatusCode::OK);

    let spec = common::RecipeSpec {
        modified_at: 2_100,
        ..Default::default()
    };
    let (bytes, same_doc, _) = common::build_recipe_bundle_with(&author, &spec);
    assert_eq!(same_doc, doc_id);
    let response = h.publish(&token, bytes.clone()).await;
    response.assert_status(StatusCode::CONFLICT);
    assert_code(&response, code::REMOVED);
    assert_eq!(response.json::<Value>()["reason"], "illegal");

    delete(&h, &admin, &format!("/admin/takedowns/{doc_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    h.publish(&token, bytes)
        .await
        .assert_status(StatusCode::CREATED);
}

#[tokio::test]
async fn a_revision_takedown_leaves_the_rest_of_the_document_served() {
    let (h, admin) = moderated().await;
    let author = SecretKeySet::from_seed(&[132u8; 32]);
    let token = h.register_and_login(&author, "author").await;
    let (doc_id, first) = published(&h, &author, &token, 2_000).await;
    let (_, second) = published(&h, &author, &token, 2_100).await;

    let takedown = json!({ "doc_id": doc_id, "revision": second, "reason": "spam" });
    post(&h, &admin, "/admin/takedowns", &takedown)
        .await
        .assert_status(StatusCode::OK);
    let conflict = post(&h, &admin, "/admin/takedowns", &takedown).await;
    conflict.assert_status(StatusCode::CONFLICT);
    assert_code(&conflict, code::CONFLICT);

    h.client
        .get(&format!("/api/v1/bundles/{doc_id}/{second}"))
        .send()
        .await
        .assert_status(StatusCode::GONE);
    for path in [
        format!("/api/v1/bundles/{doc_id}"),
        format!("/api/v1/bundles/{doc_id}/{first}"),
    ] {
        h.client
            .get(&path)
            .send()
            .await
            .assert_status(StatusCode::OK);
    }
    // A new revision of the document may still be published.
    published(&h, &author, &token, 2_200).await;
    assert_eq!(
        audit_trail(&h),
        [entry(
            "takedown",
            &format!("{doc_id}/{second}"),
            Some("spam")
        )]
    );
}

#[tokio::test]
async fn suspension_ends_sessions_and_blocks_login_and_registration_until_lifted() {
    let (h, admin) = moderated().await;
    let author = SecretKeySet::from_seed(&[133u8; 32]);
    let usr_id = author.user_id().unwrap().0;
    let token = h.register_and_login(&author, "suspect").await;
    let (doc_id, _) = published(&h, &author, &token, 2_000).await;
    put_bundle(&h, "rcp1second", "rev1x", &usr_id, 50);

    let body = json!({ "reason": "harassment", "take_down_all": true });
    let response = post(
        &h,
        &admin,
        &format!("/admin/accounts/{usr_id}/suspend"),
        &body,
    )
    .await;
    response.assert_status(StatusCode::OK);
    let view: Value = response.json();
    assert_eq!(view["sessions_ended"], 1);
    let mut taken: Vec<String> = serde_json::from_value(view["taken_down"].clone()).unwrap();
    taken.sort();
    let mut expected = vec![doc_id.clone(), "rcp1second".to_owned()];
    expected.sort();
    assert_eq!(taken, expected);
    assert!(h.storage.inner.lock().unwrap().bundles.is_empty());

    get(&h, &token, "/account/usage")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    let response = sign_in(&h, &author, "suspect").await;
    response.assert_status(StatusCode::FORBIDDEN);
    assert_code(&response, code::ACCOUNT_SUSPENDED);

    // The suspension outlives the account: its key cannot register again.
    h.storage.delete_account(&usr_id).await.unwrap();
    let profile = json!({ "profile": signed_profile(&author, "suspect2", "Again") });
    let response = h
        .client
        .post("/api/v1/auth/register")
        .json(&profile)
        .send()
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
    assert_code(&response, code::ACCOUNT_SUSPENDED);

    post_empty(&h, &admin, &format!("/admin/accounts/{usr_id}/unsuspend"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    h.register(&author, "suspect2").await;
    sign_in(&h, &author, "suspect2")
        .await
        .assert_status(StatusCode::OK);

    let trail = audit_trail(&h);
    let actions: Vec<&str> = trail.iter().map(|(action, ..)| action.as_str()).collect();
    assert_eq!(actions, ["takedown", "takedown", "suspend", "unsuspend"]);
    assert_eq!(trail[2], entry("suspend", &usr_id, Some("harassment")));
    assert_eq!(trail[3], entry("unsuspend", &usr_id, None));
    let actors: Vec<String> = (h.storage.inner.lock().unwrap().actions.iter())
        .map(|a| a.actor.clone())
        .collect();
    assert!(
        actors
            .iter()
            .all(|a| *a == admin_key().user_id().unwrap().0)
    );
}

#[tokio::test]
async fn a_session_issued_as_a_suspension_commits_does_not_authenticate() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[134u8; 32]);
    let token = h.register_and_login(&author, "racer").await;
    // A suspension row with the session still present: what a login that
    // passed its suspension check just before `suspend` committed leaves.
    h.storage
        .inner
        .lock()
        .unwrap()
        .suspensions
        .push(suspension::Model {
            usr_id: author.user_id().unwrap().0,
            reason: "spam".to_owned(),
            actor: "usr1admin".to_owned(),
            created_at: 1,
        });
    get(&h, &token, "/account/usage")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn suspending_without_take_down_all_keeps_the_accounts_documents() {
    let (h, admin) = moderated().await;
    let author = SecretKeySet::from_seed(&[134u8; 32]);
    let usr_id = author.user_id().unwrap().0;
    let token = h.register_and_login(&author, "keeper").await;
    let (doc_id, _) = published(&h, &author, &token, 2_000).await;
    let body = json!({ "reason": "spam" });
    let path = format!("/admin/accounts/{usr_id}/suspend");
    let view: Value = post(&h, &admin, &path, &body).await.json();
    assert_eq!(view["taken_down"], json!([]));
    h.client
        .get(&format!("/api/v1/bundles/{doc_id}"))
        .send()
        .await
        .assert_status(StatusCode::OK);
    let again = post(&h, &admin, &path, &body).await;
    again.assert_status(StatusCode::CONFLICT);
    let missing = post(&h, &admin, "/admin/accounts/usr1none/suspend", &body).await;
    missing.assert_status(StatusCode::NOT_FOUND);
    let not_suspended = post_empty(&h, &admin, "/admin/accounts/usr1none/unsuspend").await;
    not_suspended.assert_status(StatusCode::NOT_FOUND);
    assert_eq!(audit_trail(&h), [entry("suspend", &usr_id, Some("spam"))]);
}

#[tokio::test]
async fn reports_are_queued_resolved_and_outlive_their_reporter() {
    let (h, admin) = moderated().await;
    let author = SecretKeySet::from_seed(&[135u8; 32]);
    let reporter = SecretKeySet::from_seed(&[136u8; 32]);
    let token = h.register_and_login(&author, "author").await;
    let reporting = h.register_and_login(&reporter, "reporter").await;
    let (doc_id, revision) = published(&h, &author, &token, 2_000).await;

    let report = json!({ "doc_id": doc_id, "revision": revision, "category": "copyright", "detail": "  my photo  " });
    let filed: Value = post(&h, &reporting, "/reports", &report).await.json();
    assert_eq!(filed["status"], "open");
    assert_eq!(filed["detail"], "my photo");
    assert_eq!(filed["reporter"], reporter.user_id().unwrap().0);
    let wrong_revision = json!({ "doc_id": doc_id, "revision": "rev1none", "category": "spam" });
    post(&h, &reporting, "/reports", &wrong_revision)
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let second = json!({ "doc_id": doc_id, "category": "other" });
    let second: Value = post(&h, &reporting, "/reports", &second).await.json();

    let id = filed["id"].as_i64().unwrap();
    let actioned = json!({ "resolution": "actioned" });
    let resolved: Value = post(
        &h,
        &admin,
        &format!("/admin/reports/{id}/resolve"),
        &actioned,
    )
    .await
    .json();
    assert_eq!(resolved["status"], "actioned");
    assert_eq!(resolved["resolved_by"], admin_key().user_id().unwrap().0);
    assert!(resolved["resolved_at"].is_i64());

    let open: Value = get(&h, &admin, "/admin/reports?status=open").await.json();
    assert_eq!(open["reports"].as_array().unwrap().len(), 1);
    assert_eq!(open["reports"][0]["id"], second["id"]);
    let all: Value = get(&h, &admin, "/admin/reports").await.json();
    assert_eq!(all["reports"].as_array().unwrap().len(), 2);
    let done: Value = get(&h, &admin, "/admin/reports?status=actioned")
        .await
        .json();
    assert_eq!(done["reports"][0]["id"], id);

    // A takedown may name the report it answers; the audit row keeps it.
    let takedown = json!({ "doc_id": doc_id, "reason": "copyright", "report_id": id });
    let tombstone: Value = post(&h, &admin, "/admin/takedowns", &takedown).await.json();
    assert_eq!(tombstone["report_id"], id);
    let report_ids: Vec<Option<i64>> = (h.storage.inner.lock().unwrap().actions.iter())
        .map(|a| a.report_id)
        .collect();
    assert_eq!(report_ids, [Some(id), Some(id)]);
    assert_eq!(
        audit_trail(&h)[0],
        entry("resolve_report", &format!("report/{id}"), Some("actioned"))
    );

    // Deleting the reporter keeps its reports, with no reporter.
    h.storage
        .delete_account(&reporter.user_id().unwrap().0)
        .await
        .unwrap();
    let all: Value = get(&h, &admin, "/admin/reports").await.json();
    for report in all["reports"].as_array().unwrap() {
        assert_eq!(report["reporter"], Value::Null);
    }
    h.client.assert_conformance();
}

#[tokio::test]
async fn server_info_advertises_the_abuse_contact_and_terms() {
    let h = Harness::new();
    let info: Value = h.client.get("/api/v1/server-info").send().await.json();
    assert_eq!(info["abuse_contact"], Value::Null);
    assert_eq!(info["terms_url"], Value::Null);

    let mut config = admin_config();
    config.moderation.abuse_contact = Some("mailto:abuse@crisp.example".to_owned());
    config.moderation.terms_url = Some("https://crisp.example/terms".to_owned());
    let h = Harness::with_config(config);
    let info: Value = h.client.get("/api/v1/server-info").send().await.json();
    assert_eq!(info["abuse_contact"], "mailto:abuse@crisp.example");
    assert_eq!(info["terms_url"], "https://crisp.example/terms");
    h.client.assert_conformance();
}
