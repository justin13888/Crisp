//! Unpublishing and account deletion over real PostgreSQL, gated on
//! `CRISP_TEST_DATABASE_URL` like `tests/postgres.rs`: the per-publisher
//! delete, the index cascade, and the account cascade the migration adds.
//! The migration itself is checked in a schema of its own.

mod common;
mod pg;

use std::sync::Arc;

use common::{RecipeSpec, build_recipe_bundle_with, now, platform, test_config};
use crisp_core::sign::SecretKeySet;
use crisp_server::db::{Migrator, bundle};
use crisp_server::state::App;
use crisp_server::storage::{PgStorage, Storage};
use kynos::http::StatusCode;
use kynos::test::{TestClient, TestResponse};
use pg::{connect, fresh_identity, log_in};
use sea_orm::ActiveValue::Set;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, Statement};
use sea_orm_migration::MigratorTrait;

/// The router over `db`.
fn client(db: DatabaseConnection) -> TestClient<App> {
    let storage: Arc<dyn Storage> = Arc::new(PgStorage::new(db));
    let config = test_config();
    let service = crisp_server::router(&config.http)
        .build(App::new(storage, Arc::new(platform()), Arc::new(config)))
        .expect("router builds");
    TestClient::new(service)
}

/// A fresh, logged-in account: `(key, usr… id, token)`.
async fn account(client: &TestClient<App>) -> (SecretKeySet, String, String) {
    let (key, username) = fresh_identity();
    let token = log_in(client, &key, &username).await;
    let usr_id = key.user_id().unwrap().0;
    (key, usr_id, token)
}

/// Publishes a distinct revision of `author`'s test document; returns
/// `(doc_id, revision)`.
async fn publish(
    client: &TestClient<App>,
    author: &SecretKeySet,
    token: &str,
    n: u64,
) -> (String, String) {
    let spec = RecipeSpec {
        modified_at: 2_000 + n,
        ..RecipeSpec::default()
    };
    let (bytes, doc_id, revision) = build_recipe_bundle_with(author, &spec);
    client
        .post("/api/v1/bundles")
        .header("authorization", &format!("Bearer {token}"))
        .body("application/octet-stream", bytes)
        .send()
        .await
        .assert_status(StatusCode::CREATED);
    (doc_id, revision)
}

/// Stores a revision of `doc_id` as `coauthor` published it, newer than any
/// published through the router. Returns its stored bytes.
async fn coauthor_revision(db: &DatabaseConnection, doc_id: &str, coauthor: &str) -> Vec<u8> {
    let raw = format!("co-{coauthor}").into_bytes();
    bundle::Entity::insert(bundle::ActiveModel {
        content_hash: Set(format!("co-{coauthor}")),
        doc_id: Set(doc_id.to_owned()),
        revision: Set(format!("rev-co-{coauthor}")),
        kind: Set("recipe".to_owned()),
        format_version: Set(1),
        raw: Set(raw.clone()),
        size_bytes: Set(raw.len() as i64),
        published_by: Set(coauthor.to_owned()),
        published_at: Set(now() + 3600),
        endorsed_at: Set(now()),
    })
    .exec(db)
    .await
    .expect("insert a co-author's revision");
    raw
}

async fn delete(client: &TestClient<App>, token: &str, path: &str) -> TestResponse {
    client
        .delete(&format!("/api/v1/{path}"))
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await
}

/// How many rows of `table` have `column = value`.
async fn count(db: &DatabaseConnection, table: &str, column: &str, value: &str) -> i64 {
    let row = db
        .query_one(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!("SELECT COUNT(*) AS n FROM {table} WHERE {column} = $1"),
            [value.into()],
        ))
        .await
        .expect("count")
        .expect("a row");
    row.try_get("", "n").expect("n")
}

#[tokio::test]
async fn unpublishing_removes_only_the_callers_revisions_and_their_index_rows() {
    let Some(db) = connect("unpublishing_removes_only_the_callers_revisions").await else {
        return;
    };
    let client = client(db.clone());
    let (author, _, token) = account(&client).await;
    let (_, coauthor_id, coauthor_token) = account(&client).await;
    let (doc_id, first) = publish(&client, &author, &token, 0).await;
    let (_, second) = publish(&client, &author, &token, 1).await;
    assert_eq!(count(&db, "recipes_index", "doc_id", &doc_id).await, 2);

    // The newest goes; the next newest is served.
    delete(&client, &token, &format!("bundles/{doc_id}/{second}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let latest = client
        .get(&format!("/api/v1/bundles/{doc_id}"))
        .send()
        .await;
    latest.assert_status(StatusCode::OK);
    let exact = client
        .get(&format!("/api/v1/bundles/{doc_id}/{first}"))
        .send()
        .await;
    assert_eq!(latest.bytes(), exact.bytes());
    assert_eq!(count(&db, "recipes_index", "doc_id", &doc_id).await, 1);

    // A co-author's revision is theirs: refused to us, kept by ours.
    let theirs = coauthor_revision(&db, &doc_id, &coauthor_id).await;
    let co_path = format!("bundles/{doc_id}/rev-co-{coauthor_id}");
    delete(&client, &token, &co_path)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    delete(&client, &token, &format!("bundles/{doc_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(count(&db, "bundles", "doc_id", &doc_id).await, 1);
    assert_eq!(count(&db, "recipes_index", "doc_id", &doc_id).await, 0);
    let latest = client
        .get(&format!("/api/v1/bundles/{doc_id}"))
        .send()
        .await;
    assert_eq!(latest.bytes().as_ref(), theirs.as_slice());
    delete(&client, &token, &format!("bundles/{doc_id}"))
        .await
        .assert_status(StatusCode::FORBIDDEN);

    delete(&client, &coauthor_token, &format!("bundles/{doc_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    delete(&client, &coauthor_token, &format!("bundles/{doc_id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    client
        .get(&format!("/api/v1/bundles/{doc_id}"))
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_an_account_cascades_to_every_row_it_owns() {
    let Some(db) = connect("deleting_an_account_cascades_to_every_row_it_owns").await else {
        return;
    };
    let client = client(db.clone());
    let (author, usr_id, token) = account(&client).await;
    let (_, coauthor_id, _) = account(&client).await;
    let (doc_id, _) = publish(&client, &author, &token, 0).await;
    publish(&client, &author, &token, 1).await;
    let theirs = coauthor_revision(&db, &doc_id, &coauthor_id).await;
    assert_eq!(count(&db, "bundles", "published_by", &usr_id).await, 2);

    delete(&client, &token, "account")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    for (table, column) in [
        ("accounts", "usr_id"),
        ("sessions", "usr_id"),
        ("auth_challenges", "usr_id"),
        ("bundles", "published_by"),
    ] {
        assert_eq!(count(&db, table, column, &usr_id).await, 0, "{table}");
    }
    assert_eq!(count(&db, "recipes_index", "doc_id", &doc_id).await, 0);
    let latest = client
        .get(&format!("/api/v1/bundles/{doc_id}"))
        .send()
        .await;
    assert_eq!(latest.bytes().as_ref(), theirs.as_slice());
    delete(&client, &token, "account")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_stale_session_cannot_delete_its_account() {
    let Some(db) = connect("a_stale_session_cannot_delete_its_account").await else {
        return;
    };
    let client = client(db.clone());
    let (_, usr_id, token) = account(&client).await;
    db.execute(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE sessions SET created_at = created_at - 601 WHERE usr_id = $1",
        [usr_id.as_str().into()],
    ))
    .await
    .expect("age the session");

    delete(&client, &token, "account")
        .await
        .assert_status(StatusCode::FORBIDDEN);
    assert_eq!(count(&db, "accounts", "usr_id", &usr_id).await, 1);
}

/// Whether deleting the `usrold` account, which published a bundle,
/// succeeds; the attempt runs in a transaction that is always rolled back.
async fn account_deletes(db: &DatabaseConnection) -> bool {
    db.execute_unprepared("BEGIN").await.expect("begin");
    let deleted = db
        .execute_unprepared("DELETE FROM accounts WHERE usr_id = 'usrold'")
        .await
        .is_ok();
    db.execute_unprepared("ROLLBACK").await.expect("rollback");
    deleted
}

#[tokio::test]
async fn the_account_deletion_migration_cascades_and_reverts() {
    let Some(db) = connect("the_account_deletion_migration_cascades_and_reverts").await else {
        return;
    };
    // A schema of its own, so reverting leaves the shared one alone.
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

    Migrator::up(&scoped, Some(2))
        .await
        .expect("earlier schema");
    scoped
        .execute_unprepared(
            "INSERT INTO accounts VALUES ('usrold', 'old', 'Old', '', '', '', 0); \
             INSERT INTO bundles (content_hash, doc_id, revision, kind, format_version, \
                 raw, size_bytes, published_by, published_at, endorsed_at) \
             VALUES ('old', 'docold', 'rev', 'recipe', 1, '\\x01', 1, 'usrold', 0, 0)",
        )
        .await
        .expect("an account with a bundle");
    assert!(
        !account_deletes(&scoped).await,
        "the old key does not cascade"
    );

    Migrator::up(&scoped, Some(1))
        .await
        .expect("deletion migration");
    assert!(account_deletes(&scoped).await, "the key cascades");

    Migrator::down(&scoped, Some(1)).await.expect("revert");
    assert!(!account_deletes(&scoped).await, "reverted");

    db.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .expect("drop schema");
}
