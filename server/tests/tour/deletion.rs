//! The responses `DELETE /bundles/{doc_id}[/{revision}]` and
//! `DELETE /account` declare, and the helpers that send them.

use std::sync::Arc;

use crisp_core::sign::SecretKeySet;
use crisp_server::error::code;
use kynos::http::StatusCode;
use kynos::test::{Coverage, TestResponse};

use crate::common::{
    Fault, Harness, MockStorage, RecipeSpec, assert_code, build_recipe_bundle_with, now,
    test_config,
};

/// Sends an authenticated `DELETE /bundles/{target}`, where `target` is a
/// document id or `doc_id/revision`.
pub async fn unpublish(h: &Harness, token: &str, target: &str) -> TestResponse {
    h.client
        .delete(&format!("/api/v1/bundles/{target}"))
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await
}

/// Sends an authenticated `DELETE /account`.
pub async fn delete_account(h: &Harness, token: &str) -> TestResponse {
    h.client
        .delete("/api/v1/account")
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await
}

/// Drives the three operations through 204, 400, 401, 403, 404, and 500;
/// their 503 is in [`super::outage`].
pub async fn deletion_responses(coverage: &Coverage) {
    let storage = Arc::new(MockStorage::default());
    let h = Harness::covered(test_config(), Arc::clone(&storage), coverage);
    let author = SecretKeySet::from_seed(&[95u8; 32]);
    let stranger = SecretKeySet::from_seed(&[96u8; 32]);
    let token = h.register_and_login(&author, "unpublisher").await;
    let other = h.register_and_login(&stranger, "stranger").await;
    let mut revisions = Vec::new();
    for modified_at in [2_000, 2_100] {
        let spec = RecipeSpec {
            modified_at,
            ..RecipeSpec::default()
        };
        let (bytes, doc_id, revision) = build_recipe_bundle_with(&author, &spec);
        h.publish(&token, bytes)
            .await
            .assert_status(StatusCode::CREATED);
        revisions.push((doc_id, revision));
    }
    let (doc_id, first) = revisions.swap_remove(0);

    for target in [format!("{doc_id}/{first}"), doc_id.clone()] {
        unpublish(&h, "not-a-token", &target)
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
        unpublish(&h, &token, &format!("{target}%FF"))
            .await
            .assert_status(StatusCode::BAD_REQUEST);
        unpublish(&h, &token, &format!("{target}x"))
            .await
            .assert_status(StatusCode::NOT_FOUND);
        let response = unpublish(&h, &other, &target).await;
        response.assert_status(StatusCode::FORBIDDEN);
        assert_code(&response, code::FORBIDDEN);
        storage.inject("unpublish", Fault::Db);
        let response = unpublish(&h, &token, &target).await;
        response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
        assert_code(&response, code::INTERNAL);
        storage.heal();
    }
    unpublish(&h, &token, &format!("{doc_id}/{first}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    unpublish(&h, &token, &doc_id)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert!(storage.inner.lock().unwrap().bundles.is_empty());

    let usr_id = author.user_id().unwrap().0;
    storage.put_session("stale-token", &usr_id, now() + 60);
    let response = delete_account(&h, "stale-token").await;
    response.assert_status(StatusCode::FORBIDDEN);
    assert_code(&response, code::REAUTHENTICATION_REQUIRED);
    storage.inject("delete_account", Fault::Db);
    let response = delete_account(&h, &token).await;
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    assert_code(&response, code::INTERNAL);
    storage.heal();
    delete_account(&h, &token)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    delete_account(&h, &token)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    h.client.assert_conformance();
}
