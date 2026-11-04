//! PostgreSQL test support: the gated connection and a real login, shared by
//! the test binaries that run against `CRISP_TEST_DATABASE_URL`. Kept out of
//! `common`, which `crisp-wasm`'s tests also include, so that crate needs no
//! migration or database dependencies.

use crisp_core::sign::SecretKeySet;
use crisp_server::state::App;
use kynos::http::StatusCode;
use kynos::test::TestClient;
use sea_orm::DatabaseConnection;
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tokio::sync::OnceCell;

use crate::common::signed_profile;

/// Ensures migrations ran once per test binary.
static MIGRATED: OnceCell<()> = OnceCell::const_new();

/// Connects to the test database (migrated), or `None` to skip.
///
/// # Panics
///
/// When `CRISP_TEST_DATABASE_URL` is unset but `CI` is set: CI must never
/// pass these tests by skipping them.
pub async fn connect(test: &str) -> Option<DatabaseConnection> {
    let Ok(url) = std::env::var("CRISP_TEST_DATABASE_URL") else {
        assert!(
            std::env::var_os("CI").is_none(),
            "{test}: CI is set but CRISP_TEST_DATABASE_URL is not; \
             the PostgreSQL tests must run in CI"
        );
        eprintln!("{test} skipped: set CRISP_TEST_DATABASE_URL to run it");
        return None;
    };
    let db = sea_orm::Database::connect(&url).await.expect("connect");
    MIGRATED
        .get_or_init(|| async {
            crisp_server::db::Migrator::up(&db, None)
                .await
                .expect("migrate");
        })
        .await;
    Some(db)
}

/// A fresh random key and a unique name derived from it.
pub fn fresh_identity() -> (SecretKeySet, String) {
    let seed: [u8; 32] = rand::random();
    (
        SecretKeySet::from_seed(&seed),
        format!("pg{}", &hex::encode(seed)[..12]),
    )
}

/// Registers `username` and logs in through the real handlers, returning the
/// session token.
pub async fn log_in(client: &TestClient<App>, author: &SecretKeySet, username: &str) -> String {
    client
        .post("/api/v1/auth/register")
        .json(&json!({ "profile": signed_profile(author, username, "PG") }))
        .send()
        .await
        .assert_status(StatusCode::OK);
    let challenge: Value = client
        .post("/api/v1/auth/challenge")
        .json(&json!({ "username": username }))
        .send()
        .await
        .json();
    let nonce = hex::decode(challenge["nonce"].as_str().unwrap()).unwrap();
    let message =
        crisp_core::sign::login_message(challenge["audience"].as_str().unwrap(), &nonce).unwrap();
    let signature = author.sign(&message).unwrap();
    let login: Value = client
        .post("/api/v1/auth/login")
        .json(&json!({ "nonce": challenge["nonce"], "signature": hex::encode(&signature) }))
        .send()
        .await
        .json();
    login["token"].as_str().expect("token").to_owned()
}
