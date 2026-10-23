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

use common::{MockStorage, platform, signed_profile, test_config};
use crisp_core::sign::SecretKeySet;
use crisp_server::error::{PROBLEM_BASE, code};
use crisp_server::state::App;
use crisp_server::storage::Storage;
use crisp_wasm::api::{self, types};
use kynos::server::Server;
use serde_json::Value;

/// Serves the real router on `127.0.0.1:0`; returns a client for it.
async fn serve() -> api::Client {
    let config = test_config();
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
