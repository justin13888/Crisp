//! The HTTP gateway against the real server: `cargo test -p crisp-mcp --test server`.
//!
//! Each test serves `crisp_server`'s router — real handlers, real
//! verification and endorsement — on an ephemeral loopback port over the
//! server tests' in-memory `MockStorage`, then publishes, downloads, and
//! searches through [`HttpGateway`] exactly as `crisp-mcp` does.

#[path = "../../server/tests/common/mod.rs"]
mod common;

use std::sync::Arc;

use common::{MockStorage, platform, signed_profile, test_config};
use crisp_core::model::primitives::Timestamp;
use crisp_core::sign::SecretKeySet;
use crisp_core::verify::signatures::PlatformKey;
use crisp_core::verify::{Trust, verify};
use crisp_mcp::api::{self, types};
use crisp_mcp::clock::FixedClock;
use crisp_mcp::config::ServerConfig;
use crisp_mcp::draft::RecipeDraft;
use crisp_mcp::gateway::{HttpGateway, SearchQuery};
use crisp_mcp::library::{Library, MemoryLibrary};
use crisp_mcp::signer::Signer;
use crisp_mcp::tools::{Authoring, SaveOutcome};
use crisp_server::state::App;
use crisp_server::storage::Storage;
use kynos::server::Server;

/// Serves the real router on `127.0.0.1:0`; returns its base URL.
async fn serve() -> String {
    serve_over(Arc::new(MockStorage::default())).await
}

/// [`serve`] over `storage`, so a test can inspect what the server stored.
async fn serve_over(storage: Arc<MockStorage>) -> String {
    let config = test_config();
    let storage: Arc<dyn Storage> = storage;
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
    format!("http://{address}")
}

/// Registers `author` as `username`, as the user would in the web app.
async fn register(url: &str, author: &SecretKeySet, username: &str) {
    api::Client::new(url)
        .expect("client")
        .register(&types::RegisterRequest {
            profile: signed_profile(author, username, "Cook"),
        })
        .await
        .expect("registers");
}

fn authoring(url: &str, author: SecretKeySet, username: &str) -> (Arc<MemoryLibrary>, Authoring) {
    let signer = Arc::new(Signer::from_secret(author));
    let gateway = HttpGateway::new(
        ServerConfig {
            url: url.to_owned(),
            username: username.to_owned(),
        },
        Arc::clone(&signer),
    )
    .expect("gateway");
    let library = Arc::new(MemoryLibrary::default());
    let tools = Authoring::new(
        library.clone(),
        Arc::new(gateway),
        signer,
        Some("Cook".into()),
        Arc::new(FixedClock(Timestamp(common::now().unsigned_abs()))),
    );
    (library, tools)
}

fn draft() -> RecipeDraft {
    serde_json::from_value(serde_json::json!({
        "language": "en",
        "title": "Cacio e pepe",
        "yield": { "amount": "2", "unit_text": "servings" },
        "ingredients": [
            { "ref": 1, "name": "tonnarelli", "quantity": "200 g" },
            { "ref": 2, "name": "Pecorino Romano", "quantity": "80 g" }
        ],
        "steps": [{ "text": "Toss the pasta with the cheese and pepper.", "ingredients": [{ "ref": 1 }, { "ref": 2 }] }],
        "cuisine": ["italian"],
        "attribution": "Drafted with AI-assisted research from [a source](https://example.com/cacio)."
    }))
    .expect("draft")
}

#[tokio::test]
async fn a_saved_recipe_publishes_endorsed_and_is_found_and_downloaded() {
    let url = serve().await;
    let author = SecretKeySet::from_seed(&[21u8; 32]);
    register(&url, &author, "cook").await;
    let (library, tools) = authoring(&url, author, "cook");

    let saved = tools.save(&draft(), None, None).await.expect("saves");
    let published = tools.publish(&saved.doc_id).await.expect("publishes");
    assert_eq!(published.server, url);

    let stored = library
        .read(&saved.doc_id)
        .await
        .expect("read")
        .expect("stored");
    let pinned = PlatformKey {
        keyset: platform().public_keyset().clone(),
        valid_from: Timestamp(0),
        valid_until: None,
    };
    let report = verify(&stored, &[pinned], Timestamp(common::now().unsigned_abs()));
    assert_eq!(report.trust, Trust::Endorsed, "{report:#?}");

    let hits = tools
        .gateway
        .search(SearchQuery {
            q: Some("cacio".into()),
            ..SearchQuery::default()
        })
        .await
        .expect("searches");
    assert!(
        hits.iter()
            .any(|h| h.doc_id == saved.doc_id && h.title == "Cacio e pepe"),
        "{hits:?}"
    );

    let downloaded = tools
        .gateway
        .download(&saved.doc_id)
        .await
        .expect("downloads");
    assert_eq!(downloaded.as_deref(), Some(stored.as_slice()));
    let absent = saved.doc_id.replace('q', "p");
    assert_eq!(
        tools
            .gateway
            .download(&absent)
            .await
            .expect("absent is not an error"),
        None
    );
}

#[tokio::test]
async fn another_cook_forks_a_published_recipe_from_the_server() {
    let url = serve().await;
    let author = SecretKeySet::from_seed(&[22u8; 32]);
    register(&url, &author, "original").await;
    let (_, original) = authoring(&url, author, "original");
    let saved = original.save(&draft(), None, None).await.expect("saves");
    original.publish(&saved.doc_id).await.expect("publishes");

    let (_, other) = authoring(&url, SecretKeySet::from_seed(&[23u8; 32]), "unregistered");
    let got = other
        .get(&saved.doc_id)
        .await
        .expect("reads from the server");
    assert_eq!(got.found_in, "server");
    let forked = other
        .save(&got.draft, Some(&saved.doc_id), None)
        .await
        .expect("forks");
    assert_eq!(forked.outcome, SaveOutcome::Forked);

    let refused = other
        .publish(&forked.doc_id)
        .await
        .expect_err("not registered");
    assert!(
        refused.contains("Register it in the Crisp app"),
        "{refused}"
    );
}

#[tokio::test]
async fn an_ended_session_is_replaced_by_signing_in_again() {
    let url = serve().await;
    let author = SecretKeySet::from_seed(&[26u8; 32]);
    register(&url, &author, "returning").await;
    let (_, tools) = authoring(&url, SecretKeySet::from_seed(&[26u8; 32]), "returning");
    let first = tools.save(&draft(), None, None).await.expect("saves");
    tools
        .publish(&first.doc_id)
        .await
        .expect("publishes, signing in");

    // End every session of the account from elsewhere, as the web app's
    // "sign out everywhere" does.
    let client = api::Client::new(&url).expect("client");
    let challenge = client
        .challenge(&types::ChallengeRequest {
            username: "returning".into(),
        })
        .await
        .expect("challenge")
        .into_inner();
    let nonce = hex::decode(&challenge.nonce).expect("hex");
    let message = crisp_core::sign::login_message(&challenge.audience, &nonce).expect("message");
    let session = client
        .login(&types::LoginRequest {
            nonce: challenge.nonce,
            signature: hex::encode(author.sign(&message).expect("sign")),
        })
        .await
        .expect("login")
        .into_inner();
    client
        .with_credential(
            "sessionToken",
            api::Credential::Bearer(secrecy::SecretString::from(session.token)),
        )
        .logout_all()
        .await
        .expect("signs out everywhere");

    let mut edited = draft();
    edited.title = "Cacio e pepe, revised".into();
    let second = tools
        .save(&edited, Some(&first.doc_id), Some(&first.revision))
        .await
        .expect("revises");
    let published = tools
        .publish(&second.doc_id)
        .await
        .expect("signs in again and publishes");
    assert_eq!(published.revision, second.revision);
}

#[tokio::test]
async fn a_refused_upload_reports_the_servers_answer_without_signing_in_again() {
    let storage = Arc::new(MockStorage::default());
    let url = serve_over(Arc::clone(&storage)).await;
    let author = SecretKeySet::from_seed(&[27u8; 32]);
    register(&url, &author, "refused").await;
    let (_, tools) = authoring(&url, author, "refused");
    let error = tools
        .gateway
        .publish(b"not a crisp file".to_vec())
        .await
        .expect_err("refused");
    assert!(error.0.contains("the server answered 4"), "{error}");
    let sessions = storage.inner.lock().expect("storage").sessions.len();
    assert_eq!(sessions, 1, "only a refused session earns a second sign-in");
}

#[tokio::test]
async fn the_wrong_key_for_a_username_is_explained() {
    let url = serve().await;
    register(&url, &SecretKeySet::from_seed(&[24u8; 32]), "owner").await;
    let (_, impostor) = authoring(&url, SecretKeySet::from_seed(&[25u8; 32]), "owner");
    let saved = impostor
        .save(&draft(), None, None)
        .await
        .expect("saves locally");
    let refused = impostor
        .publish(&saved.doc_id)
        .await
        .expect_err("wrong key");
    assert!(refused.contains("CRISP_KEY_FILE"), "{refused}");
}
