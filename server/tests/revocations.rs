//! Endorsement revocation over the real router (spec §8.8): a takedown
//! revokes the endorsement of every bundle it removes, `GET /revocations`
//! serves the list signed by the current platform key, and lifting the
//! takedown does not restore it.

mod common;
mod tour;

use common::{Fault, Harness, RecipeSpec, assert_code, build_recipe_bundle_with, now, platform};
use crisp_core::codec::decode_file;
use crisp_core::crypto::hash::sha256;
use crisp_core::model::primitives::Timestamp;
use crisp_core::sign::SecretKeySet;
use crisp_core::verify::{Revocation, Trust, accept_revocations, verify_with_revocations};
use crisp_server::error::code;
use crisp_server::revocations::Revocations;
use kynos::http::StatusCode;
use serde_json::json;
use tour::moderation::{admin_config, admin_key, delete, post};

/// Fetches `GET /revocations` and accepts it as a client that pinned this
/// server's key set would.
async fn revocations(h: &Harness) -> Vec<Revocation> {
    let response = h.client.get("/api/v1/revocations").send().await;
    response.assert_status(StatusCode::OK);
    let body: Revocations = response.json();
    let list: Vec<Revocation> = (body.revocations.iter())
        .map(|r| Revocation {
            content_hash: hex::decode(&r.content_hash).expect("hex hash"),
            revoked_at: Timestamp(r.revoked_at as u64),
        })
        .collect();
    let signers: Vec<&str> = body.signatures.iter().map(|s| s.usr_id.as_str()).collect();
    assert_eq!(signers, [platform().user_id()]);
    let sigs: Vec<Vec<u8>> = (body.signatures.iter())
        .map(|s| hex::decode(&s.sig).expect("hex signature"))
        .collect();
    accept_revocations(&platform().platform_keys(), &[], &list, &sigs).expect("vouched")
}

#[tokio::test]
async fn a_takedown_revokes_the_endorsement_and_lifting_it_does_not_restore_it() {
    let h = Harness::with_config(admin_config());
    let admin = h.register_and_login(&admin_key(), "admin").await;
    let author = SecretKeySet::from_seed(&[140u8; 32]);
    let user = h.register_and_login(&author, "author").await;
    let (bytes, doc_id, revision) = build_recipe_bundle_with(&author, &RecipeSpec::default());
    let response = h.publish(&user, bytes.clone()).await;
    response.assert_status(StatusCode::CREATED);
    let endorsed = response.bytes().to_vec();
    assert!(revocations(&h).await.is_empty());

    let before = now();
    let takedown = json!({ "doc_id": doc_id, "revision": revision, "reason": "illegal" });
    post(&h, &admin, "/admin/takedowns", &takedown)
        .await
        .assert_status(StatusCode::OK);
    let revoked = revocations(&h).await;
    let payload = decode_file(&endorsed)
        .expect("decode")
        .signed_bundle
        .payload;
    assert_eq!(revoked.len(), 1);
    assert_eq!(revoked[0].content_hash, sha256(&payload).to_vec());
    assert!(revoked[0].revoked_at.0 >= before as u64);

    let at = Timestamp(now() as u64);
    let report = verify_with_revocations(&endorsed, &platform().platform_keys(), &revoked, at);
    assert_eq!(report.trust, Trust::AuthorSigned);
    let codes: Vec<Option<&str>> = (report.failures().iter())
        .map(|c| c.code.as_deref())
        .collect();
    assert_eq!(codes, [Some("E.SIG.REVOKED")]);

    delete(&h, &admin, &format!("/admin/takedowns/{doc_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(revocations(&h).await, revoked);

    // The same bundle is never endorsed again; a new revision is.
    let response = h.publish(&user, bytes).await;
    response.assert_status(StatusCode::CONFLICT);
    assert_code(&response, code::CONFLICT);
    let spec = RecipeSpec {
        modified_at: 3_000,
        ..RecipeSpec::default()
    };
    let (bytes, _, _) = build_recipe_bundle_with(&author, &spec);
    h.publish(&user, bytes)
        .await
        .assert_status(StatusCode::CREATED);
}

#[tokio::test]
async fn a_failed_revocation_check_refuses_the_publish() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[142u8; 32]);
    let user = h.register_and_login(&author, "author").await;
    h.storage.inject("is_revoked", Fault::Db);
    let (bytes, _, _) = build_recipe_bundle_with(&author, &RecipeSpec::default());
    let response = h.publish(&user, bytes).await;
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    assert!(h.storage.inner.lock().unwrap().bundles.is_empty());
}

#[tokio::test]
async fn suspending_with_a_takedown_revokes_every_document_of_the_account() {
    let h = Harness::with_config(admin_config());
    let admin = h.register_and_login(&admin_key(), "admin").await;
    let author = SecretKeySet::from_seed(&[141u8; 32]);
    let user = h.register_and_login(&author, "spammer").await;
    let usr_id = author.user_id().expect("usr id").0;
    for modified_at in [2_000, 3_000] {
        let spec = RecipeSpec {
            modified_at,
            title: format!("Spam {modified_at}"),
            ..RecipeSpec::default()
        };
        let (bytes, _, _) = build_recipe_bundle_with(&author, &spec);
        h.publish(&user, bytes)
            .await
            .assert_status(StatusCode::CREATED);
    }
    let suspend = json!({ "reason": "spam", "take_down_all": true });
    post(
        &h,
        &admin,
        &format!("/admin/accounts/{usr_id}/suspend"),
        &suspend,
    )
    .await
    .assert_status(StatusCode::OK);
    assert_eq!(revocations(&h).await.len(), 2);
}
