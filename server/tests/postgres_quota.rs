//! Storage quotas over real PostgreSQL, gated on `CRISP_TEST_DATABASE_URL`
//! like `tests/postgres.rs`: the stored size, the usage sums, the server-wide
//! cap, that racing publishes cannot exceed a quota together, and the quota
//! migration itself.

mod common;
mod pg;

use std::sync::Arc;

use common::{RecipeSpec, build_recipe_bundle_with, platform, test_config};
use crisp_core::sign::SecretKeySet;
use crisp_server::db::{Migrator, bundle};
use crisp_server::error::code;
use crisp_server::quota::{AccountUsage, Breach, Quota};
use crisp_server::state::App;
use crisp_server::storage::{BundleInsertError, PgStorage, Storage};
use kynos::http::StatusCode;
use kynos::test::{TestClient, TestResponse};
use pg::{connect, fresh_identity, log_in};
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, QueryFilter,
    Statement,
};
use sea_orm_migration::MigratorTrait;
use serde_json::Value;

/// The router over `db`, enforcing `quota`, with a fresh logged-in author.
async fn start(db: DatabaseConnection, quota: Quota) -> (TestClient<App>, SecretKeySet, String) {
    let storage: Arc<dyn Storage> = Arc::new(PgStorage::new(db));
    let config = crisp_server::config::Config {
        quota,
        ..test_config()
    };
    let service = crisp_server::router(&config.http)
        .build(App::new(storage, Arc::new(platform()), Arc::new(config)))
        .expect("router builds");
    let client = TestClient::new(service);
    let (author, username) = fresh_identity();
    let token = log_in(&client, &author, &username).await;
    (client, author, token)
}

/// A distinct revision by `author`.
fn revision(author: &SecretKeySet, n: u64) -> Vec<u8> {
    let spec = RecipeSpec {
        modified_at: 2_000 + n,
        ..RecipeSpec::default()
    };
    build_recipe_bundle_with(author, &spec).0
}

async fn publish(client: &TestClient<App>, token: &str, bytes: Vec<u8>) -> TestResponse {
    client
        .post("/api/v1/bundles")
        .header("authorization", &format!("Bearer {token}"))
        .body("application/octet-stream", bytes)
        .send()
        .await
}

async fn usage(client: &TestClient<App>, token: &str) -> AccountUsage {
    let response = client
        .get("/api/v1/account/usage")
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await;
    response.assert_status(StatusCode::OK);
    response.json()
}

#[tokio::test]
async fn usage_sums_the_stored_endorsed_sizes() {
    let Some(db) = connect("usage_sums_the_stored_endorsed_sizes").await else {
        return;
    };
    let (client, author, token) = start(db.clone(), Quota::default()).await;
    assert_eq!(usage(&client, &token).await.bundles_used, 0);
    let mut total = 0;
    for n in 0..2 {
        let response = publish(&client, &token, revision(&author, n)).await;
        response.assert_status(StatusCode::CREATED);
        total += response.bytes().len() as u64;
    }
    let after = usage(&client, &token).await;
    assert_eq!((after.bytes_used, after.bundles_used), (total, 2));

    // The stored size is the length of the stored bytes, as the migration
    // backfills it.
    let usr_id = author.user_id().unwrap().0;
    let rows = bundle::Entity::find()
        .filter(bundle::Column::PublishedBy.eq(usr_id.as_str()))
        .all(&db)
        .await
        .expect("bundles");
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row.size_bytes, row.raw.len() as i64);
    }
    let index = db
        .query_one_raw(sea_orm::Statement::from_string(
            db.get_database_backend(),
            "SELECT 1 FROM pg_indexes WHERE indexname = 'bundles_published_by_idx'",
        ))
        .await
        .expect("query");
    assert!(index.is_some(), "the published_by index exists");
    client.assert_conformance();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn racing_publishes_cannot_exceed_the_bundle_quota() {
    let Some(db) = connect("racing_publishes_cannot_exceed_the_bundle_quota").await else {
        return;
    };
    let quota = Quota {
        bundles_per_account: 2,
        ..Quota::default()
    };
    let (client, author, token) = start(db, quota).await;
    // One bundle already stored, so the quota has room for exactly one more.
    publish(&client, &token, revision(&author, 0))
        .await
        .assert_status(StatusCode::CREATED);
    // Four racers, built before any is sent so they reach the server
    // together; without the row lock several of them would fit.
    let [r1, r2, r3, r4] = [1, 2, 3, 4].map(|n| revision(&author, n));
    let responses = tokio::join!(
        publish(&client, &token, r1),
        publish(&client, &token, r2),
        publish(&client, &token, r3),
        publish(&client, &token, r4),
    );
    let responses = [responses.0, responses.1, responses.2, responses.3];
    let mut statuses: Vec<u16> = responses.iter().map(|r| r.status().as_u16()).collect();
    statuses.sort_unstable();
    assert_eq!(statuses, [201, 403, 403, 403]);
    for refused in responses
        .iter()
        .filter(|r| r.status() == StatusCode::FORBIDDEN)
    {
        let body: Value = refused.json();
        assert_eq!(body["code"], code::QUOTA_EXCEEDED);
        assert_eq!(
            (&body["quota"], &body["used"]),
            (&"bundles".into(), &2.into())
        );
    }
    assert_eq!(usage(&client, &token).await.bundles_used, 2);
    client.assert_conformance();
}

#[tokio::test]
async fn the_server_cap_refuses_with_507_and_stores_nothing() {
    let Some(db) = connect("the_server_cap_refuses_with_507_and_stores_nothing").await else {
        return;
    };
    let full = Quota {
        max_stored_bytes: Some(1),
        ..Quota::default()
    };
    let (client, author, token) = start(db, full).await;
    let refused = publish(&client, &token, revision(&author, 0)).await;
    refused.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    let body: Value = refused.json();
    assert_eq!(body["code"], code::STORAGE_FULL);
    assert_eq!(usage(&client, &token).await.bundles_used, 0);
    client.assert_conformance();
}

/// A minimal bundle row of `size` bytes published by `usr_id`; the check
/// reads only `published_by` and `size_bytes`.
fn row(usr_id: &str, size: i64) -> bundle::ActiveModel {
    let key = hex::encode(rand::random::<[u8; 16]>());
    bundle::ActiveModel {
        content_hash: Set(key.clone()),
        doc_id: Set(format!("doc{key}")),
        revision: Set("rev".to_owned()),
        kind: Set("recipe".to_owned()),
        format_version: Set(1),
        raw: Set(vec![0; usize::try_from(size).unwrap()]),
        size_bytes: Set(size),
        published_by: Set(usr_id.to_owned()),
        published_at: Set(0),
        endorsed_at: Set(0),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn racing_inserts_are_checked_one_after_another() {
    let Some(db) = connect("racing_inserts_are_checked_one_after_another").await else {
        return;
    };
    let (_client, author, _token) = start(db.clone(), Quota::default()).await;
    let usr_id = author.user_id().unwrap().0;
    let storage = Arc::new(PgStorage::new(db));
    let quota = Quota {
        bytes_per_account: 100,
        ..Quota::default()
    };
    // Eight inserts of 40 bytes against 100: exactly two fit, whatever the
    // interleaving, because each waits on the account row lock.
    let racers: Vec<_> = (0..8)
        .map(|_| {
            let (storage, bundle) = (Arc::clone(&storage), row(&usr_id, 40));
            tokio::spawn(async move { storage.insert_bundle(bundle, Vec::new(), &quota).await })
        })
        .collect();
    let mut stored = 0;
    for racer in racers {
        match racer.await.expect("racer") {
            Ok(()) => stored += 1,
            Err(BundleInsertError::Quota(Breach::Account { used, .. })) => assert_eq!(used, 80),
            Err(other) => panic!("unexpected failure: {other}"),
        }
    }
    assert_eq!(stored, 2);
    let usage = storage.account_usage(&usr_id).await.expect("usage");
    assert_eq!((usage.bytes, usage.bundles), (80, 2));
}

#[tokio::test]
async fn the_server_cap_counts_what_is_already_stored() {
    let Some(db) = connect("the_server_cap_counts_what_is_already_stored").await else {
        return;
    };
    let (_client, author, _token) = start(db.clone(), Quota::default()).await;
    let usr_id = author.user_id().unwrap().0;
    let storage = PgStorage::new(db);
    storage
        .insert_bundle(row(&usr_id, 40), Vec::new(), &Quota::default())
        .await
        .expect("the first insert fits");
    // The second 40 bytes fit a 41-byte cap on their own; only the 40 (or
    // more, from other tests) already stored push them over.
    let capped = Quota {
        max_stored_bytes: Some(41),
        ..Quota::default()
    };
    match storage
        .insert_bundle(row(&usr_id, 40), Vec::new(), &capped)
        .await
    {
        Err(BundleInsertError::Quota(Breach::Server {
            limit,
            used,
            requested,
        })) => {
            assert_eq!((limit, requested), (41, 40));
            assert!(used >= 40, "the total includes the stored bundle: {used}");
        }
        Ok(()) => panic!("an insert over the server cap was admitted"),
        Err(other) => panic!("unexpected failure: {other}"),
    }
}

/// Whether `sql`, bound to `schema`, returns a row on `db`.
async fn exists(db: &DatabaseConnection, sql: &str, schema: &str) -> bool {
    db.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        sql,
        [schema.into()],
    ))
    .await
    .expect("query")
    .is_some()
}

const SIZE_COLUMN: &str = "SELECT 1 FROM information_schema.columns \
     WHERE table_schema = $1 AND table_name = 'bundles' AND column_name = 'size_bytes'";
const PUBLISHER_INDEX: &str = "SELECT 1 FROM pg_indexes \
     WHERE schemaname = $1 AND indexname = 'bundles_published_by_idx'";

#[tokio::test]
async fn the_quota_migration_backfills_sizes_and_reverts() {
    let Some(db) = connect("the_quota_migration_backfills_sizes_and_reverts").await else {
        return;
    };
    // A schema of its own, so reverting the migration leaves the shared one
    // every other test uses alone.
    let schema = format!("migration_{}", hex::encode(rand::random::<[u8; 6]>()));
    db.execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .expect("create schema");
    let url = std::env::var("CRISP_TEST_DATABASE_URL").expect("connect checked it");
    let mut options = sea_orm::ConnectOptions::new(url);
    options
        .max_connections(1)
        .set_schema_search_path(schema.clone());
    let scoped = sea_orm::Database::connect(options).await.expect("connect");

    // A bundle stored before the quota migration existed.
    Migrator::up(&scoped, Some(1))
        .await
        .expect("initial schema");
    assert!(!exists(&scoped, SIZE_COLUMN, &schema).await);
    scoped
        .execute_unprepared(
            "INSERT INTO accounts VALUES ('usrold', 'old', 'Old', '', '', '', 0); \
             INSERT INTO bundles (content_hash, doc_id, revision, kind, format_version, \
                 raw, published_by, published_at, endorsed_at) \
             VALUES ('old', 'docold', 'rev', 'recipe', 1, '\\x0102030405', 'usrold', 0, 0)",
        )
        .await
        .expect("a pre-quota bundle");

    Migrator::up(&scoped, Some(1))
        .await
        .expect("quota migration");
    let old = bundle::Entity::find_by_id("old")
        .one(&scoped)
        .await
        .expect("query")
        .expect("the bundle survives");
    assert_eq!(old.size_bytes, 5, "backfilled from the stored bytes");
    assert!(exists(&scoped, PUBLISHER_INDEX, &schema).await);

    Migrator::down(&scoped, Some(1)).await.expect("revert");
    assert!(!exists(&scoped, SIZE_COLUMN, &schema).await);
    assert!(!exists(&scoped, PUBLISHER_INDEX, &schema).await);

    db.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .expect("drop schema");
}
