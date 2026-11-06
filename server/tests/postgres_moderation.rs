//! Moderation over real PostgreSQL, gated on `CRISP_TEST_DATABASE_URL` like
//! `tests/postgres.rs`: takedowns and their tombstones, the report cap and
//! the reporter becoming `NULL` on deletion, suspensions outliving their
//! account, and the append-only audit trail.
//!
//! Every test uses fresh identities and documents, so they share one
//! database with the other PostgreSQL tests.

mod common;
mod pg;

use std::sync::Arc;

use common::{RecipeSpec, build_recipe_bundle_with, now, platform, test_config};
use crisp_core::sign::SecretKeySet;
use crisp_server::db::{Migrator, report};
use crisp_server::state::App;
use crisp_server::storage::{
    FileReport, Lift, Moderation, NewReport, NewSuspension, NewTakedown, PgStorage, Resolve,
    Storage, Suspend, TakeDown,
};
use kynos::http::StatusCode;
use kynos::test::TestClient;
use pg::{connect, fresh_identity, log_in};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, Statement};
use sea_orm_migration::MigratorTrait;

/// The router over `db`, with no administrator configured.
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

/// Publishes `author`'s test document; returns `(doc_id, revision)`.
async fn publish(client: &TestClient<App>, author: &SecretKeySet, token: &str) -> (String, String) {
    let (bytes, doc_id, revision) = build_recipe_bundle_with(author, &RecipeSpec::default());
    client
        .post("/api/v1/bundles")
        .header("authorization", &format!("Bearer {token}"))
        .body("application/octet-stream", bytes)
        .send()
        .await
        .assert_status(StatusCode::CREATED);
    (doc_id, revision)
}

/// How many rows of `table` have `column = value`.
async fn count(db: &DatabaseConnection, table: &str, column: &str, value: &str) -> i64 {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!("SELECT COUNT(*) AS n FROM {table} WHERE {column} = $1"),
            [value.into()],
        ))
        .await
        .expect("count")
        .expect("a row");
    row.try_get("", "n").expect("n")
}

fn takedown(actor: &str, doc_id: &str, revision: Option<&str>) -> NewTakedown {
    NewTakedown {
        actor: actor.to_owned(),
        doc_id: doc_id.to_owned(),
        revision: revision.map(str::to_owned),
        reason: "copyright".to_owned(),
        report_id: None,
        at: now(),
    }
}

#[tokio::test]
async fn a_takedown_deletes_the_bundles_and_index_rows_and_leaves_a_tombstone() {
    let Some(db) = connect("a_takedown_deletes_the_bundles").await else {
        return;
    };
    let c = client(db.clone());
    let (author, usr_id, token) = account(&c).await;
    let (doc_id, revision) = publish(&c, &author, &token).await;
    let storage = PgStorage::new(db.clone());

    let outcome = storage
        .take_down(takedown("usr1admin", &doc_id, None))
        .await
        .unwrap();
    let TakeDown::Removed { tombstone, audit } = outcome else {
        panic!("expected a takedown, got {outcome:?}");
    };
    assert_eq!(tombstone.content_hashes.as_array().map(Vec::len), Some(1));
    assert_eq!(
        (audit.action.as_str(), audit.target.as_str()),
        ("takedown", doc_id.as_str())
    );
    assert_eq!(count(&db, "bundles", "doc_id", &doc_id).await, 0);
    assert_eq!(count(&db, "recipes_index", "doc_id", &doc_id).await, 0);
    assert_eq!(storage.tombstones(&doc_id).await.unwrap().len(), 1);
    c.get(&format!("/api/v1/bundles/{doc_id}/{revision}"))
        .send()
        .await
        .assert_status(StatusCode::GONE);

    let again = storage
        .take_down(takedown("usr1admin", &doc_id, Some(&revision)))
        .await;
    assert!(matches!(again.unwrap(), TakeDown::AlreadyRemoved(_)));
    let missing = storage
        .take_down(takedown("usr1admin", &format!("{doc_id}x"), None))
        .await;
    assert_eq!(missing.unwrap(), TakeDown::Absent);

    let lifted = storage
        .lift_takedown(&doc_id, "usr1admin", now())
        .await
        .unwrap();
    assert!(matches!(lifted, Lift::Lifted { tombstones: 1, .. }));
    assert_eq!(
        storage
            .lift_takedown(&doc_id, "usr1admin", now())
            .await
            .unwrap(),
        Lift::Absent
    );
    assert_eq!(count(&db, "moderation_actions", "target", &doc_id).await, 2);
    // The publisher may publish the document again.
    publish(&c, &author, &token).await;
    assert_eq!(count(&db, "bundles", "published_by", &usr_id).await, 1);
}

#[tokio::test]
async fn the_audit_trail_refuses_updates_and_deletes() {
    let Some(db) = connect("the_audit_trail_refuses_updates_and_deletes").await else {
        return;
    };
    let storage = PgStorage::new(db.clone());
    let (_, username) = fresh_identity();
    let usr_id = format!("usr1{username}");
    assert_eq!(
        storage
            .unsuspend(&usr_id, "usr1admin", now())
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        storage
            .suspend(NewSuspension {
                actor: "usr1admin".to_owned(),
                usr_id: usr_id.clone(),
                reason: "spam".to_owned(),
                take_down_all: false,
                at: now(),
            })
            .await
            .unwrap(),
        Suspend::Absent
    );
    // Any audit row will do; write one through a lift of a fresh tombstone.
    let c = client(db.clone());
    let (author, _, token) = account(&c).await;
    let (doc_id, _) = publish(&c, &author, &token).await;
    let TakeDown::Removed { audit, .. } = storage
        .take_down(takedown("usr1admin", &doc_id, None))
        .await
        .unwrap()
    else {
        panic!("expected a takedown");
    };
    for sql in [
        "UPDATE moderation_actions SET reason = 'edited' WHERE id = $1",
        "DELETE FROM moderation_actions WHERE id = $1",
    ] {
        let refused = db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                [audit.id.into()],
            ))
            .await;
        let error = refused.expect_err(sql).to_string();
        assert!(error.contains("append-only"), "{sql}: {error}");
    }
}

#[tokio::test]
async fn reports_are_capped_resolved_once_and_outlive_their_reporter() {
    let Some(db) = connect("reports_are_capped_resolved_once").await else {
        return;
    };
    let c = client(db.clone());
    let (author, usr_id, token) = account(&c).await;
    let (doc_id, _) = publish(&c, &author, &token).await;
    let storage = PgStorage::new(db.clone());
    let report = || NewReport {
        reporter: usr_id.clone(),
        doc_id: doc_id.clone(),
        revision: None,
        category: "spam".to_owned(),
        detail: String::new(),
        at: now(),
    };
    let FileReport::Filed(first) = storage.file_report(report(), 2).await.unwrap() else {
        panic!("the first report is filed");
    };
    assert!(matches!(
        storage.file_report(report(), 2).await.unwrap(),
        FileReport::Filed(_)
    ));
    assert_eq!(
        storage.file_report(report(), 2).await.unwrap(),
        FileReport::TooMany
    );

    let resolved = storage
        .resolve_report(first.id, "dismissed", "usr1admin", now())
        .await;
    let Resolve::Resolved {
        report: closed,
        audit,
    } = resolved.unwrap()
    else {
        panic!("the report resolves");
    };
    assert_eq!(closed.status, "dismissed");
    assert_eq!(audit.report_id, Some(first.id));
    let again = storage
        .resolve_report(first.id, "actioned", "usr1admin", now())
        .await;
    assert!(matches!(again.unwrap(), Resolve::AlreadyResolved(_)));
    assert_eq!(
        storage
            .resolve_report(-1, "dismissed", "usr1admin", now())
            .await
            .unwrap(),
        Resolve::Absent
    );
    // A resolved report no longer counts against the cap.
    assert!(matches!(
        storage.file_report(report(), 2).await.unwrap(),
        FileReport::Filed(_)
    ));
    let open = storage.reports(Some("open"), 200, 0).await.unwrap();
    assert!(open.iter().all(|r| r.status == "open"));

    storage.delete_account(&usr_id).await.unwrap();
    assert_eq!(count(&db, "reports", "reporter", &usr_id).await, 0);
    let kept = report::Entity::find_by_id(first.id).one(&db).await.unwrap();
    assert_eq!(kept.expect("the report stays").reporter, None);
}

#[tokio::test]
async fn a_suspension_ends_sessions_takes_documents_down_and_outlives_the_account() {
    let Some(db) = connect("a_suspension_ends_sessions").await else {
        return;
    };
    let c = client(db.clone());
    let (author, usr_id, token) = account(&c).await;
    let (doc_id, _) = publish(&c, &author, &token).await;
    let storage = PgStorage::new(db.clone());
    let suspension = || NewSuspension {
        actor: "usr1admin".to_owned(),
        usr_id: usr_id.clone(),
        reason: "spam".to_owned(),
        take_down_all: true,
        at: now(),
    };

    let outcome = storage.suspend(suspension()).await.unwrap();
    let Suspend::Suspended {
        sessions,
        takedowns,
        audit,
    } = outcome
    else {
        panic!("expected a suspension, got {outcome:?}");
    };
    assert_eq!(sessions, 1);
    assert_eq!(takedowns.len(), 1);
    assert_eq!(takedowns[0].doc_id, doc_id);
    let actions: Vec<&str> = audit.iter().map(|a| a.action.as_str()).collect();
    assert_eq!(actions, ["takedown", "suspend"]);
    assert_eq!(count(&db, "sessions", "usr_id", &usr_id).await, 0);
    assert_eq!(count(&db, "bundles", "doc_id", &doc_id).await, 0);
    assert_eq!(
        storage.suspend(suspension()).await.unwrap(),
        Suspend::AlreadySuspended
    );

    storage.delete_account(&usr_id).await.unwrap();
    assert!(storage.suspension(&usr_id).await.unwrap().is_some());
    assert_eq!(count(&db, "moderation_actions", "target", &usr_id).await, 1);
    let lifted = storage
        .unsuspend(&usr_id, "usr1admin", now())
        .await
        .unwrap();
    assert_eq!(lifted.map(|a| a.action), Some("unsuspend".to_owned()));
    assert!(storage.suspension(&usr_id).await.unwrap().is_none());
}

#[tokio::test]
async fn a_session_left_beside_a_suspension_does_not_authenticate() {
    let Some(db) = connect("a_session_left_beside_a_suspension").await else {
        return;
    };
    let c = client(db.clone());
    let (_, usr_id, token) = account(&c).await;
    // The suspension row alone, its session kept: what a login that passed
    // its suspension check just before `suspend` committed leaves behind.
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO suspensions (usr_id, reason, actor, created_at) VALUES ($1, 'spam', 'usr1admin', $2)",
        [usr_id.clone().into(), now().into()],
    ))
    .await
    .unwrap();
    assert_eq!(count(&db, "sessions", "usr_id", &usr_id).await, 1);
    let storage = PgStorage::new(db);
    assert!(storage.valid_session(&token).await.unwrap().is_none());
}

#[tokio::test]
async fn the_moderation_migration_applies_and_reverts() {
    let Some(db) = connect("the_moderation_migration_applies_and_reverts").await else {
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
    let tables = |scoped: &DatabaseConnection| {
        let sql = format!(
            "SELECT COUNT(*) AS n FROM information_schema.tables WHERE table_schema = '{schema}' \
             AND table_name IN ('reports', 'takedowns', 'suspensions', 'moderation_actions')"
        );
        let scoped = scoped.clone();
        async move {
            let row = scoped
                .query_one_raw(Statement::from_string(DbBackend::Postgres, sql))
                .await
                .expect("count tables")
                .expect("a row");
            row.try_get::<i64>("", "n").expect("n")
        }
    };

    Migrator::up(&scoped, None).await.expect("every migration");
    assert_eq!(tables(&scoped).await, 4);
    Migrator::down(&scoped, Some(1)).await.expect("revert");
    assert_eq!(tables(&scoped).await, 0);
    Migrator::up(&scoped, None).await.expect("reapply");
    assert_eq!(tables(&scoped).await, 4);

    db.execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .expect("drop schema");
}
