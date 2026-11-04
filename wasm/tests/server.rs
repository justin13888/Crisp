//! The spargen-generated client against the real server, natively:
//! `cargo test -p crisp-wasm --test server`.
//!
//! Each test serves `crisp_server`'s router — real handlers, real
//! verification — on an ephemeral loopback port over the server tests'
//! in-memory `MockStorage`, then drives [`crisp_wasm::api::Client`] at it
//! over HTTP, so a drift between the router and the client the web app is
//! built on fails here rather than in a browser.
#![cfg(not(target_arch = "wasm32"))]

#[path = "../../server/tests/common/mod.rs"]
mod common;

use std::sync::Arc;

use common::{MockStorage, build_recipe_bundle, platform, signed_profile, test_config};
use crisp_core::sign::SecretKeySet;
use crisp_server::error::{PROBLEM_BASE, code};
use crisp_server::quota::Quota;
use crisp_server::state::App;
use crisp_server::storage::Storage;
use crisp_wasm::api::{self, types};
use kynos::server::Server;
use serde_json::Value;

/// Serves the real router on `127.0.0.1:0`; returns a client for it.
async fn serve() -> api::Client {
    serve_with(test_config()).await
}

/// Serves the real router under `config`; returns a client for it.
async fn serve_with(config: crisp_server::config::Config) -> api::Client {
    let storage: Arc<dyn Storage> = Arc::new(MockStorage::default());
    let service = crisp_server::router(&config.http)
        .build(App::new(storage, Arc::new(platform()), Arc::new(config)))
        .expect("router builds");
    let bound = Server::new(service)
        .bind("127.0.0.1:0")
        .prepare()
        .await
        .expect("an ephemeral port binds");
    let address = bound.local_addrs()[0];
    tokio::spawn(bound.serve());
    api::Client::new(&format!("http://{address}")).expect("client builds")
}

/// A string member of a documented problem body, as the generated type
/// keeps it (it declares `type` and `detail`, not the server's `code`).
fn problem_member<T: serde::Serialize>(body: &T, member: &str) -> Option<String> {
    serde_json::to_value(body)
        .ok()?
        .get(member)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[tokio::test]
async fn server_info_keys_decode_to_the_platform_keyset() {
    let client = serve().await;
    let info = client
        .server_info()
        .await
        .expect("server info")
        .into_inner();

    assert_eq!(info.api_versions, vec!["v1"]);
    assert_eq!(
        u64::try_from(info.format_version.current).ok(),
        Some(crisp_core::FORMAT_VERSION)
    );
    let platform = platform();
    let expected = platform.public_keyset();
    assert_eq!(info.platform_keys.len(), 1);
    let key = &info.platform_keys[0];
    assert_eq!(key.usr_id, platform.user_id());
    assert_eq!(
        hex::decode(&key.ed25519).ok().as_ref(),
        Some(&expected.ed25519)
    );
    assert_eq!(
        hex::decode(&key.ml_dsa_65).ok().as_ref(),
        Some(&expected.ml_dsa_65)
    );
}

#[tokio::test]
async fn registering_a_taken_username_is_a_409_conflict() {
    let client = serve().await;
    let author = SecretKeySet::from_seed(&[7u8; 32]);
    let request = types::RegisterRequest {
        profile: signed_profile(&author, "alice", "Alice"),
    };

    let registered = client
        .register(&request)
        .await
        .expect("first registration")
        .into_inner();
    assert_eq!(registered.usr_id, author.user_id().expect("author id").0);

    // A different key under the same username, so the key-already-registered
    // check passes and the username-taken branch is the one that answers.
    let rival = SecretKeySet::from_seed(&[8u8; 32]);
    let rival_request = types::RegisterRequest {
        profile: signed_profile(&rival, "alice", "Another Alice"),
    };
    let error = client
        .register(&rival_request)
        .await
        .expect_err("a second author under the same username conflicts");
    assert_eq!(error.status().map(|s| s.as_u16()), Some(409));
    match error {
        api::Error::Api(value) => match value.inner() {
            api::RegisterError::Status409(body) => {
                assert_eq!(
                    problem_member(body, "type"),
                    Some(format!("{PROBLEM_BASE}{}", code::CONFLICT))
                );
                assert!(
                    problem_member(body, "detail").is_some_and(|d| d.contains("username")),
                    "the client surfaces the username-taken detail, not the key conflict"
                );
            }
            other => panic!("expected the documented 409, got {other:?}"),
        },
        other => panic!("expected a documented API error, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unknown_document_downloads_as_a_404() {
    let client = serve().await;

    let latest = client
        .get_latest("doc1none".to_owned())
        .await
        .expect_err("no latest revision");
    assert_eq!(latest.status().map(|s| s.as_u16()), Some(404));
    assert!(matches!(
        latest,
        api::Error::Api(ref value) if matches!(value.inner(), api::GetLatestError::Status404(_))
    ));

    let exact = client
        .get_exact("doc1none".to_owned(), "rev1none".to_owned())
        .await
        .expect_err("no exact revision");
    assert_eq!(exact.status().map(|s| s.as_u16()), Some(404));
    assert!(matches!(
        exact,
        api::Error::Api(ref value) if matches!(value.inner(), api::GetExactError::Status404(_))
    ));
}

/// Registers `author` as `username` and signs in; returns a client carrying
/// the session.
async fn signed_in(client: api::Client, author: &SecretKeySet, username: &str) -> api::Client {
    let profile = signed_profile(author, username, "Quota");
    client
        .register(&types::RegisterRequest { profile })
        .await
        .expect("register");
    let challenge = client
        .challenge(&types::ChallengeRequest {
            username: username.to_owned(),
        })
        .await
        .expect("challenge")
        .into_inner();
    let nonce = hex::decode(&challenge.nonce).expect("hex nonce");
    let message = crisp_core::sign::login_message(&challenge.audience, &nonce).expect("message");
    let signature = hex::encode(author.sign(&message).expect("sign"));
    let session = client
        .login(&types::LoginRequest {
            nonce: challenge.nonce,
            signature,
        })
        .await
        .expect("login")
        .into_inner();
    client.with_credential(
        "sessionToken",
        api::Credential::Bearer(secrecy::SecretString::from(session.token)),
    )
}

#[tokio::test]
async fn logout_all_reaches_the_server_and_ends_the_session() {
    let author = SecretKeySet::from_seed(&[10u8; 32]);
    let client = signed_in(serve().await, &author, "everywhere").await;
    let listed = client.sessions().await.expect("sessions").into_inner();
    assert_eq!(listed.sessions.len(), 1);
    assert!(listed.sessions[0].current);

    client.logout_all().await.expect("logout everywhere");
    let error = client
        .account_usage()
        .await
        .expect_err("the session has ended");
    assert_eq!(error.status().map(|s| s.as_u16()), Some(401));
}

#[tokio::test]
async fn usage_and_the_quota_refusal_reach_the_client_typed() {
    let config = crisp_server::config::Config {
        quota: Quota {
            bytes_per_account: 1,
            ..Quota::default()
        },
        ..test_config()
    };
    let author = SecretKeySet::from_seed(&[9u8; 32]);
    let client = signed_in(serve_with(config).await, &author, "quota").await;

    let usage = client.account_usage().await.expect("usage").into_inner();
    assert_eq!((usage.bytes_used, usage.bytes_limit), (0, 1));
    assert_eq!((usage.bundles_used, usage.bundles_limit), (0, 5000));

    let (bundle, _) = build_recipe_bundle(&author);
    let error = client
        .publish(&bytes::Bytes::from(bundle))
        .await
        .expect_err("the bundle does not fit one byte");
    assert_eq!(error.status().map(|s| s.as_u16()), Some(403));
    match error {
        api::Error::Api(value) => match value.inner() {
            api::PublishError::Status403(body) => {
                // The problem name the engine turns into the failure `code`.
                assert_eq!(
                    problem_member(body, "type"),
                    Some(format!("{PROBLEM_BASE}quota-exceeded"))
                );
                assert!(
                    problem_member(body, "detail")
                        .is_some_and(|d| d.contains("delete old revisions")),
                    "the detail tells the publisher how to make room"
                );
            }
            other => panic!("expected the documented 403, got {other:?}"),
        },
        other => panic!("expected a documented API error, got {other:?}"),
    }
}

#[tokio::test]
async fn unpublishing_and_account_deletion_reach_the_server() {
    let server = serve().await;
    let author = SecretKeySet::from_seed(&[11u8; 32]);
    let stranger = SecretKeySet::from_seed(&[12u8; 32]);
    let client = signed_in(server.clone(), &author, "unpublisher").await;
    let other = signed_in(server, &stranger, "stranger").await;
    let (bundle, doc_id) = build_recipe_bundle(&author);
    client
        .publish(&bytes::Bytes::from(bundle))
        .await
        .expect("publish");

    // Another account may not remove it, and learns why from the type.
    let error = other
        .unpublish_document(doc_id.clone())
        .await
        .expect_err("not the publisher");
    assert_eq!(error.status().map(|s| s.as_u16()), Some(403));
    match error {
        api::Error::Api(value) => match value.inner() {
            api::UnpublishDocumentError::Status403(body) => assert_eq!(
                problem_member(body, "type"),
                Some(format!("{PROBLEM_BASE}forbidden"))
            ),
            other => panic!("expected the documented 403, got {other:?}"),
        },
        other => panic!("expected a documented API error, got {other:?}"),
    }

    client
        .unpublish_document(doc_id.clone())
        .await
        .expect("unpublish");
    let gone = client
        .unpublish_revision(doc_id.clone(), "rev1none".to_owned())
        .await
        .expect_err("nothing left");
    assert_eq!(gone.status().map(|s| s.as_u16()), Some(404));
    assert!(matches!(
        client.get_latest(doc_id).await,
        Err(api::Error::Api(ref value))
            if matches!(value.inner(), api::GetLatestError::Status404(_))
    ));

    // A session just issued is fresh enough to delete its account.
    client.delete_account().await.expect("delete the account");
    let error = client
        .account_usage()
        .await
        .expect_err("the account is gone");
    assert_eq!(error.status().map(|s| s.as_u16()), Some(401));
}
