//! Unpublishing and account deletion over the real router: a publisher
//! removes only what it published, a co-author's revisions keep a document
//! served, and deleting an account needs a recent sign-in and leaves no row
//! of that account behind.

mod common;
mod tour;

use common::{Harness, RecipeSpec, assert_code, build_recipe_bundle_with, now, signed_profile};
use crisp_core::sign::SecretKeySet;
use crisp_server::auth::deletion::REAUTH_WINDOW_SECS;
use crisp_server::db::bundle;
use crisp_server::error::code;
use kynos::http::StatusCode;
use serde_json::json;
use tour::deletion::{delete_account, unpublish};

/// Stores a bundle row as if `published_by` had published it at
/// `published_at`, so co-authored documents need no multi-author signing.
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

/// The bytes `GET /bundles/{doc_id}` serves, or `None` on 404.
async fn latest(h: &Harness, doc_id: &str) -> Option<Vec<u8>> {
    let response = h
        .client
        .get(&format!("/api/v1/bundles/{doc_id}"))
        .send()
        .await;
    if response.status() == StatusCode::NOT_FOUND {
        return None;
    }
    response.assert_status(StatusCode::OK);
    Some(response.bytes().to_vec())
}

/// Two accounts that both publish revisions of `doc1shared`: `owner`'s at
/// 10 and 30, `coauthor`'s at 20. Returns `(owner token, coauthor token)`.
async fn shared_document(h: &Harness) -> (String, String) {
    let owner = SecretKeySet::from_seed(&[110u8; 32]);
    let coauthor = SecretKeySet::from_seed(&[111u8; 32]);
    let owner_token = h.register_and_login(&owner, "owner").await;
    let coauthor_token = h.register_and_login(&coauthor, "coauthor").await;
    let (owner_id, coauthor_id) = (owner.user_id().unwrap().0, coauthor.user_id().unwrap().0);
    put_bundle(h, "doc1shared", "rev1old", &owner_id, 10);
    put_bundle(h, "doc1shared", "rev1co", &coauthor_id, 20);
    put_bundle(h, "doc1shared", "rev1new", &owner_id, 30);
    (owner_token, coauthor_token)
}

#[tokio::test]
async fn unpublishing_the_newest_revision_serves_the_next_newest() {
    let h = Harness::new();
    let (owner, _) = shared_document(&h).await;
    assert_eq!(latest(&h, "doc1shared").await.unwrap(), b"rev1new");

    let response = unpublish(&h, &owner, "doc1shared/rev1new").await;
    response.assert_status(StatusCode::NO_CONTENT);
    assert_eq!(latest(&h, "doc1shared").await.unwrap(), b"rev1co");
    h.client
        .get("/api/v1/bundles/doc1shared/rev1new")
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
    h.client.assert_conformance();
}

#[tokio::test]
async fn a_revision_is_removed_only_by_its_publisher() {
    let h = Harness::new();
    let (owner, coauthor) = shared_document(&h).await;

    let response = unpublish(&h, &coauthor, "doc1shared/rev1new").await;
    response.assert_status(StatusCode::FORBIDDEN);
    assert_code(&response, code::FORBIDDEN);
    let response = unpublish(&h, &owner, "doc1shared/rev1co").await;
    response.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(h.storage.inner.lock().unwrap().bundles.len(), 3);

    for missing in ["doc1shared/rev1none", "doc1none/rev1new"] {
        let response = unpublish(&h, &owner, missing).await;
        response.assert_status(StatusCode::NOT_FOUND);
        assert_code(&response, code::NOT_FOUND);
    }
    unpublish(&h, "not-a-token", "doc1shared/rev1new")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    h.client.assert_conformance();
}

#[tokio::test]
async fn unpublishing_a_document_removes_only_the_callers_revisions() {
    let h = Harness::new();
    let (owner, coauthor) = shared_document(&h).await;

    unpublish(&h, &owner, "doc1shared")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let left: Vec<String> = (h.storage.inner.lock().unwrap().bundles.iter())
        .map(|b| b.revision.clone())
        .collect();
    assert_eq!(left, ["rev1co"]);
    assert_eq!(latest(&h, "doc1shared").await.unwrap(), b"rev1co");

    // Nothing of the caller's is left: refused, not "not found".
    let response = unpublish(&h, &owner, "doc1shared").await;
    response.assert_status(StatusCode::FORBIDDEN);
    assert_code(&response, code::FORBIDDEN);

    // Once no revision remains the document answers 404.
    unpublish(&h, &coauthor, "doc1shared")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert!(latest(&h, "doc1shared").await.is_none());
    let response = unpublish(&h, &coauthor, "doc1shared").await;
    response.assert_status(StatusCode::NOT_FOUND);
    assert_code(&response, code::NOT_FOUND);
    h.client.assert_conformance();
}

#[tokio::test]
async fn an_unpublished_document_can_be_published_again_and_leaves_no_index_rows() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[112u8; 32]);
    let token = h.register_and_login(&author, "republisher").await;
    let (bytes, doc_id, _) = build_recipe_bundle_with(&author, &RecipeSpec::default());
    h.publish(&token, bytes.clone())
        .await
        .assert_status(StatusCode::CREATED);
    assert!(!h.storage.inner.lock().unwrap().recipes.is_empty());

    unpublish(&h, &token, &doc_id)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert!(h.storage.inner.lock().unwrap().recipes.is_empty());
    assert!(latest(&h, &doc_id).await.is_none());
    h.publish(&token, bytes)
        .await
        .assert_status(StatusCode::CREATED);
}

#[tokio::test]
async fn deleting_an_account_removes_every_row_of_it_and_keeps_coauthors_revisions() {
    let h = Harness::new();
    let (owner, _) = shared_document(&h).await;
    let owner_key = SecretKeySet::from_seed(&[110u8; 32]);
    let owner_id = owner_key.user_id().unwrap().0;
    h.challenge("owner").await; // an outstanding login challenge
    h.storage.put_session("other-device", &owner_id, now() + 60);

    delete_account(&h, &owner)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    {
        let inner = h.storage.inner.lock().unwrap();
        assert!(inner.accounts.iter().all(|a| a.usr_id != owner_id));
        assert!(inner.sessions.iter().all(|s| s.usr_id != owner_id));
        assert!(inner.challenges.iter().all(|c| c.usr_id != owner_id));
        assert!(inner.bundles.iter().all(|b| b.published_by != owner_id));
        assert_eq!(inner.bundles.len(), 1, "the co-author's revision stays");
    }
    assert_eq!(latest(&h, "doc1shared").await.unwrap(), b"rev1co");
    delete_account(&h, &owner)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    // The username is free at once, for any key.
    let newcomer = SecretKeySet::from_seed(&[113u8; 32]);
    h.client
        .post("/api/v1/auth/register")
        .json(&json!({ "profile": signed_profile(&newcomer, "owner", "New") }))
        .send()
        .await
        .assert_status(StatusCode::OK);
    h.client.assert_conformance();
}

#[tokio::test]
async fn deleting_an_account_needs_a_recent_sign_in() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[114u8; 32]);
    let usr_id = h.register(&author, "stale").await;
    h.storage.put_session("stale-token", &usr_id, now() + 60);
    // `put_session` issues the session at epoch 0: long past the window.
    assert!(now() > REAUTH_WINDOW_SECS);

    let response = delete_account(&h, "stale-token").await;
    response.assert_status(StatusCode::FORBIDDEN);
    assert_code(&response, code::REAUTHENTICATION_REQUIRED);
    assert_eq!(h.storage.inner.lock().unwrap().accounts.len(), 1);

    // Signing in again yields a session that may delete the account.
    let (nonce, audience) = h.challenge("stale").await;
    let fresh: serde_json::Value = h.login(&author, &nonce, &audience).await.json();
    delete_account(&h, fresh["token"].as_str().unwrap())
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert!(h.storage.inner.lock().unwrap().accounts.is_empty());
    h.client.assert_conformance();
}
