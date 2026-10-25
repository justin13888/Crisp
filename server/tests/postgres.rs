//! PostgreSQL-backed tests, gated on `CRISP_TEST_DATABASE_URL`.
//!
//! These exercise the real SQL in [`PgStorage`]: the migration, full-text and
//! faceted search, the atomic challenge redemption, and unique-constraint
//! classification. Locally, each is skipped (passing) when the variable is
//! unset; where `CI` is set, a missing variable fails every test instead, so
//! the SQL can never go untested in CI:
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
use crisp_core::model::recipe::AllergenCode;
use crisp_core::sign::SecretKeySet;
use crisp_server::db::{account, challenge};
use crisp_server::search::{MAX_LIMIT, RecipePage};
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
///
/// # Panics
///
/// When `CRISP_TEST_DATABASE_URL` is unset but `CI` is set: CI must never
/// pass these tests by skipping them.
async fn connect(test: &str) -> Option<DatabaseConnection> {
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
fn fresh_identity() -> (SecretKeySet, String) {
    let seed: [u8; 32] = rand::random();
    (
        SecretKeySet::from_seed(&seed),
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

/// The real router over PostgreSQL, with a freshly registered, logged-in
/// author.
struct ApiSession {
    client: TestClient<App>,
    author: SecretKeySet,
    username: String,
    token: String,
}

impl ApiSession {
    /// Builds the router over `db`, then registers and logs in a fresh
    /// author through the real handlers.
    async fn start(db: DatabaseConnection) -> Self {
        let storage: Arc<dyn Storage> = Arc::new(PgStorage::new(db));
        let config = test_config();
        let service = crisp_server::router(&config.http)
            .build(App::new(storage, Arc::new(platform()), Arc::new(config)))
            .expect("router builds");
        let client = TestClient::new(service);
        let (author, username) = fresh_identity();
        let token = log_in(&client, &author, &username).await;
        Self {
            client,
            author,
            username,
            token,
        }
    }

    /// Publishes a recipe bundle built from `spec`, expecting 201.
    async fn publish(&self, spec: &RecipeSpec) {
        let (bytes, _, _) = build_recipe_bundle_with(&self.author, spec);
        self.client
            .post("/api/v1/bundles")
            .header("authorization", &format!("Bearer {}", self.token))
            .body("application/octet-stream", bytes)
            .send()
            .await
            .assert_status(StatusCode::CREATED);
    }

    /// Fetches a recipe page from `path`.
    async fn page(&self, path: &str) -> RecipePage {
        let response = self.client.get(path).send().await;
        response.assert_status(StatusCode::OK);
        response.json()
    }
}

/// Registers `username` and logs in through the real handlers, returning the
/// session token.
async fn log_in(client: &TestClient<App>, author: &SecretKeySet, username: &str) -> String {
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

/// Sorted titles of a page, for order-insensitive comparison.
fn titles(page: RecipePage) -> Vec<String> {
    let mut t: Vec<String> = page.recipes.into_iter().map(|r| r.title).collect();
    t.sort();
    t
}

#[tokio::test]
async fn postgres_search_through_the_api() {
    let Some(db) = connect("postgres_search_through_the_api").await else {
        return;
    };
    let session = ApiSession::start(db).await;
    let client = &session.client;

    // Publish three recipes sharing a run-unique cuisine tag.
    let tag = format!("cuisine-{}", session.username);
    for (title, extra) in [
        ("Alpha Stew", "stew"),
        ("Beta Stew", "stew"),
        ("Gamma Salad", "salad"),
    ] {
        session
            .publish(&RecipeSpec {
                title: title.into(),
                cuisine: vec![tag.clone(), extra.into()],
                ..RecipeSpec::default()
            })
            .await;
    }

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

    // Search pages: limit and offset are applied in SQL.
    let page: RecipePage = client
        .get(&format!("/api/v1/search?cuisine={tag}&limit=2&offset=2"))
        .send()
        .await
        .json();
    assert_eq!((page.limit, page.offset, page.recipes.len()), (2, 2, 1));

    client.assert_conformance();
}

#[tokio::test]
async fn allergen_free_keeps_recipes_without_the_allergen() {
    let Some(db) = connect("allergen_free_keeps_recipes_without_the_allergen").await else {
        return;
    };
    let session = ApiSession::start(db).await;
    let tag = format!("cuisine-{}", session.username);
    for (title, allergens) in [
        ("Wheat Bread", vec![AllergenCode::Gluten]),
        (
            "Cheese Toast",
            vec![AllergenCode::Gluten, AllergenCode::Milk],
        ),
        ("Rice Pudding", vec![AllergenCode::Milk]),
        ("Green Salad", vec![]),
    ] {
        session
            .publish(&RecipeSpec {
                title: title.into(),
                cuisine: vec![tag.clone()],
                allergens,
                ..RecipeSpec::default()
            })
            .await;
    }

    let search =
        |allergens: &str| format!("/api/v1/search?cuisine={tag}&allergen_free={allergens}");
    assert_eq!(
        titles(session.page(&search("gluten")).await),
        vec!["Green Salad", "Rice Pudding"]
    );
    assert_eq!(
        titles(session.page(&search("milk")).await),
        vec!["Green Salad", "Wheat Bread"]
    );
    // Every listed allergen must be absent.
    assert_eq!(
        titles(session.page(&search("gluten,milk")).await),
        vec!["Green Salad"]
    );
    // An allergen nobody declares excludes nothing.
    assert_eq!(
        titles(session.page(&search("peanuts")).await),
        vec!["Cheese Toast", "Green Salad", "Rice Pudding", "Wheat Bread"]
    );
    session.client.assert_conformance();
}

#[tokio::test]
async fn browse_orders_newest_first() {
    let Some(db) = connect("browse_orders_newest_first").await else {
        return;
    };
    let session = ApiSession::start(db).await;
    // Recent modification times (endorsement rejects a future one), published
    // out of order so insertion order cannot pass for the sort.
    let base = u64::try_from(now()).expect("positive clock") - 60;
    let suffix = &session.username;
    for (title, offset) in [("Middle", 10), ("Oldest", 0), ("Newest", 20)] {
        session
            .publish(&RecipeSpec {
                title: format!("{title} {suffix}"),
                modified_at: base + offset,
                ..RecipeSpec::default()
            })
            .await;
    }

    let page = session
        .page(&format!("/api/v1/recipes?limit={MAX_LIMIT}"))
        .await;
    // The whole page is newest-first.
    assert!(
        page.recipes
            .windows(2)
            .all(|w| w[0].modified_at >= w[1].modified_at),
        "browse is not ordered by modified_at descending"
    );
    let ours: Vec<&str> = page
        .recipes
        .iter()
        .map(|r| r.title.as_str())
        .filter(|t| t.ends_with(suffix.as_str()))
        .collect();
    assert_eq!(
        ours,
        vec![
            format!("Newest {suffix}"),
            format!("Middle {suffix}"),
            format!("Oldest {suffix}"),
        ]
    );

    // Pagination continues the same order.
    let newest = &page.recipes[0];
    let first: RecipePage = session.page("/api/v1/recipes?limit=1").await;
    assert_eq!(first.recipes[0].modified_at, newest.modified_at);
    let second: RecipePage = session.page("/api/v1/recipes?limit=1&offset=1").await;
    assert_eq!(second.offset, 1);
    assert_eq!(second.recipes[0].modified_at, page.recipes[1].modified_at);
    session.client.assert_conformance();
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
