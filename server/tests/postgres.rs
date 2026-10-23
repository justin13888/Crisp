//! PostgreSQL-backed tests, gated on `CRISP_TEST_DATABASE_URL`.
//!
//! These exercise the real SQL in [`PgStorage`]: the migration, full-text and
//! faceted search, the atomic challenge redemption, and unique-constraint
//! classification. Each is skipped (passing) when the variable is unset:
//!
//! ```bash
//! docker compose up postgres -d
//! CRISP_TEST_DATABASE_URL=postgresql://crisp:crisp@localhost:5432/crisp \
//!     cargo test -p crisp-server --test postgres
//! ```
//!
//! Every test uses freshly random keys and names, so runs never collide with
//! each other or with earlier data in the database.

mod common;

use std::sync::Arc;

use common::{RecipeSpec, build_recipe_bundle_with, now, platform, signed_profile, test_config};
use crisp_core::sign::{SecretKeySet, derive_keyset};
use crisp_server::db::{account, challenge};
use crisp_server::search::RecipePage;
use crisp_server::state::App;
use crisp_server::storage::{InsertError, PgStorage, Storage};
use kynos::http::StatusCode;
use kynos::test::TestClient;
use sea_orm::ActiveValue::Set;
use sea_orm::DatabaseConnection;
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tokio::sync::OnceCell;

/// Ensures migrations ran once per test binary.
static MIGRATED: OnceCell<()> = OnceCell::const_new();

/// Connects to the test database (migrated), or `None` to skip.
async fn connect(test: &str) -> Option<DatabaseConnection> {
    let Ok(url) = std::env::var("CRISP_TEST_DATABASE_URL") else {
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
fn fresh_identity() -> (SecretKeySet, String) {
    let seed: [u8; 32] = rand::random();
    (
        derive_keyset(&seed),
        format!("pg{}", &hex::encode(seed)[..12]),
    )
}

#[tokio::test]
async fn postgres_smoke() {
    let Some(db) = connect("postgres_smoke").await else {
        return;
    };
    // Migrations are idempotent.
    crisp_server::db::Migrator::up(&db, None)
        .await
        .expect("re-migrate");
}

#[tokio::test]
async fn postgres_search_and_browse_through_the_api() {
    let Some(db) = connect("postgres_search_and_browse_through_the_api").await else {
        return;
    };
    let storage: Arc<dyn Storage> = Arc::new(PgStorage::new(db));
    let config = test_config();
    let service = crisp_server::router(&config.http)
        .build(App::new(storage, Arc::new(platform()), Arc::new(config)))
        .expect("router builds");
    let client = TestClient::new(service);

    // Register and log in through the real handlers.
    let (author, username) = fresh_identity();
    client
        .post("/api/v1/auth/register")
        .json(&json!({ "profile": signed_profile(&author, &username, "PG") }))
        .send()
        .await
        .assert_status(StatusCode::OK);
    let challenge: Value = client
        .post("/api/v1/auth/challenge")
        .json(&json!({ "username": username }))
        .send()
        .await
        .json();
    let signature = author
        .sign(challenge["message"].as_str().unwrap().as_bytes())
        .unwrap();
    let login: Value = client
        .post("/api/v1/auth/login")
        .json(&json!({ "nonce": challenge["nonce"], "signature": hex::encode(&signature) }))
        .send()
        .await
        .json();
    let token = login["token"].as_str().expect("token").to_owned();

    // Publish three recipes sharing a run-unique cuisine tag.
    let tag = format!("cuisine-{username}");
    for (title, extra) in [
        ("Alpha Stew", "stew"),
        ("Beta Stew", "stew"),
        ("Gamma Salad", "salad"),
    ] {
        let spec = RecipeSpec {
            title: title.into(),
            cuisine: vec![tag.clone(), extra.into()],
            ..RecipeSpec::default()
        };
        let (bytes, _, _) = build_recipe_bundle_with(&author, &spec);
        client
            .post("/api/v1/bundles")
            .header("authorization", &format!("Bearer {token}"))
            .body("application/octet-stream", bytes)
            .send()
            .await
            .assert_status(StatusCode::CREATED);
    }

    let titles = |page: RecipePage| {
        let mut t: Vec<String> = page.recipes.into_iter().map(|r| r.title).collect();
        t.sort();
        t
    };
    // Facets: all listed cuisines must match (JSONB containment).
    let page: RecipePage = client
        .get(&format!("/api/v1/search?cuisine={tag}"))
        .send()
        .await
        .json();
    assert_eq!(titles(page), vec!["Alpha Stew", "Beta Stew", "Gamma Salad"]);
    let page: RecipePage = client
        .get(&format!("/api/v1/search?cuisine={tag},stew"))
        .send()
        .await
        .json();
    assert_eq!(titles(page), vec!["Alpha Stew", "Beta Stew"]);
    // Full text (tsvector) plus a facet; allergen exclusion.
    let page: RecipePage = client
        .get(&format!("/api/v1/search?q=salad&cuisine={tag}"))
        .send()
        .await
        .json();
    assert_eq!(titles(page), vec!["Gamma Salad"]);
    let page: RecipePage = client
        .get(&format!(
            "/api/v1/search?cuisine={tag}&allergen_free=gluten"
        ))
        .send()
        .await
        .json();
    assert!(page.recipes.is_empty());
    // Typed JSONB columns round-trip.
    let page: RecipePage = client
        .get(&format!("/api/v1/search?cuisine={tag}&limit=1"))
        .send()
        .await
        .json();
    assert_eq!(page.recipes.len(), 1);
    assert_eq!(page.recipes[0].authors[0].name, "Tester");
    assert_eq!(page.recipes[0].allergens, vec!["gluten"]);

    // Browse pages newest-first over everything in the database.
    let page: RecipePage = client.get("/api/v1/recipes?limit=2").send().await.json();
    assert_eq!(page.limit, 2);
    assert!(page.recipes.len() <= 2 && !page.recipes.is_empty());

    client.assert_conformance();
}

#[tokio::test]
async fn postgres_challenges_are_redeemed_once_and_duplicates_classified() {
    let Some(db) = connect("postgres_challenges_are_redeemed_once").await else {
        return;
    };
    let storage = PgStorage::new(db);
    let (author, username) = fresh_identity();
    let keyset = author.public_keyset();
    let usr_id = author.user_id().unwrap().0;
    let row = || account::ActiveModel {
        usr_id: Set(usr_id.clone()),
        username: Set(username.clone()),
        display_name: Set("PG".into()),
        ed25519_pk: Set(keyset.ed25519.clone()),
        ml_dsa_65_pk: Set(keyset.ml_dsa_65.clone()),
        signed_profile: Set(vec![1]),
        created_at: Set(now()),
    };
    storage.create_account(row()).await.expect("insert");
    assert!(matches!(
        storage.create_account(row()).await,
        Err(InsertError::Duplicate(_))
    ));

    let nonce = format!("nonce-{username}");
    storage
        .create_challenge(challenge::ActiveModel {
            nonce: Set(nonce.clone()),
            usr_id: Set(usr_id.clone()),
            created_at: Set(now()),
            expires_at: Set(now() + 60),
        })
        .await
        .expect("challenge");
    let (a, b) = tokio::join!(
        storage.take_challenge(&nonce),
        storage.take_challenge(&nonce)
    );
    let taken = [a.unwrap(), b.unwrap()];
    assert_eq!(
        taken.iter().filter(|t| t.is_some()).count(),
        1,
        "exactly one concurrent redemption wins"
    );

    // Purge removes expired sessions and challenges.
    storage
        .create_challenge(challenge::ActiveModel {
            nonce: Set(format!("expired-{username}")),
            usr_id: Set(usr_id),
            created_at: Set(0),
            expires_at: Set(1),
        })
        .await
        .expect("expired challenge");
    let counts = storage.purge_expired(now()).await.expect("purge");
    assert!(counts.challenges >= 1);
}
