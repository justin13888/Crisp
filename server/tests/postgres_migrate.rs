//! Boot migrations under the advisory lock, over real PostgreSQL, gated on
//! `CRISP_TEST_DATABASE_URL` like `tests/postgres.rs`: replicas starting
//! together against one empty database all succeed and apply the schema
//! once, and a lock held past the timeout fails the start instead of
//! hanging.
//!
//! Each test creates its own database (advisory locks are per database), so
//! it neither sees nor disturbs the shared test schema.

use std::time::Duration;

use crisp_server::config::DatabaseOptions;
use crisp_server::db::{MIGRATION_LOCK, MigrateError, Migrator, migrate};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;

/// A throwaway database, dropped by [`Scratch::drop_now`].
struct Scratch {
    /// The connection to the server's main test database, used to create
    /// and drop this one.
    admin: DatabaseConnection,
    /// This database's name.
    name: String,
    /// This database's URL.
    url: String,
}

impl Scratch {
    /// A fresh, empty database, or `None` to skip.
    ///
    /// # Panics
    ///
    /// When `CRISP_TEST_DATABASE_URL` is unset but `CI` is set.
    async fn create(test: &str) -> Option<Self> {
        let Ok(url) = std::env::var("CRISP_TEST_DATABASE_URL") else {
            assert!(
                std::env::var_os("CI").is_none(),
                "{test}: CI is set but CRISP_TEST_DATABASE_URL is not; \
                 the PostgreSQL tests must run in CI"
            );
            eprintln!("{test} skipped: set CRISP_TEST_DATABASE_URL to run it");
            return None;
        };
        let admin = sea_orm::Database::connect(&url).await.expect("connect");
        let name = format!("crisp_migrate_{}", hex::encode(rand::random::<[u8; 6]>()));
        // Unprepared (the simple protocol), since CREATE and DROP DATABASE
        // must run outside any transaction block.
        admin
            .execute_unprepared(&format!("CREATE DATABASE {name}"))
            .await
            .expect("create a scratch database");
        let (server, query) = url
            .split_once('?')
            .map_or((url.as_str(), ""), |(s, q)| (s, q));
        let (base, _) = server.rsplit_once('/').expect("a database path");
        let mut url = format!("{base}/{name}");
        if !query.is_empty() {
            url = format!("{url}?{query}");
        }
        Some(Self { admin, name, url })
    }

    /// Connects to this database.
    async fn connect(&self) -> DatabaseConnection {
        sea_orm::Database::connect(&self.url)
            .await
            .expect("connect to the scratch database")
    }

    /// Drops this database, closing any session still on it.
    async fn drop_now(self) {
        self.admin
            .execute_unprepared(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            ))
            .await
            .expect("drop the scratch database");
    }
}

/// `sql` as a raw PostgreSQL statement.
fn statement(sql: &str) -> Statement {
    Statement::from_string(DbBackend::Postgres, sql)
}

#[tokio::test]
async fn replicas_starting_together_apply_the_schema_once() {
    let Some(scratch) = Scratch::create("replicas_starting_together").await else {
        return;
    };
    let options = DatabaseOptions::default();
    let starts = (0..4).map(|_| {
        let connect = options.connect_options(&scratch.url);
        tokio::spawn(migrate(connect, Duration::from_secs(60)))
    });
    let mut applied = Vec::new();
    for start in starts.collect::<Vec<_>>() {
        applied.push(start.await.unwrap().expect("every start succeeds").applied);
    }
    let migrations = Migrator::migrations().len();
    // One replica applied everything; the others found nothing pending.
    applied.sort_unstable();
    assert_eq!(applied, [0, 0, 0, migrations]);

    let db = scratch.connect().await;
    let row = db
        .query_one_raw(statement(
            "SELECT COUNT(*) AS rows, COUNT(DISTINCT version) AS versions FROM seaql_migrations",
        ))
        .await
        .unwrap()
        .unwrap();
    let rows: i64 = row.try_get("", "rows").unwrap();
    let versions: i64 = row.try_get("", "versions").unwrap();
    assert_eq!((rows, versions), (migrations as i64, migrations as i64));
    assert!(
        Migrator::get_pending_migrations(&db)
            .await
            .unwrap()
            .is_empty()
    );
    // The lock was released: this session can take it at once.
    let held: bool = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT pg_try_advisory_lock($1) AS held",
            [MIGRATION_LOCK.into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "held")
        .unwrap();
    assert!(held, "the migration lock is released after the run");
    db.close().await.unwrap();
    scratch.drop_now().await;
}

#[tokio::test]
async fn a_held_lock_fails_the_start_after_the_timeout() {
    let Some(scratch) = Scratch::create("a_held_lock_fails_the_start").await else {
        return;
    };
    // Another session holds the lock, as a replica stuck mid-migration would.
    let mut one = sea_orm::ConnectOptions::new(&scratch.url);
    one.max_connections(1).min_connections(1);
    let holder = sea_orm::Database::connect(one).await.unwrap();
    holder
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT pg_advisory_lock($1)",
            [MIGRATION_LOCK.into()],
        ))
        .await
        .unwrap();

    let outcome = migrate(
        DatabaseOptions::default().connect_options(&scratch.url),
        Duration::from_millis(600),
    )
    .await;
    let Err(MigrateError::LockTimeout { waited }) = outcome else {
        panic!("expected a lock timeout, got {outcome:?}");
    };
    assert!(waited >= Duration::from_millis(600), "{waited:?}");
    // Nothing was applied while the lock was held.
    let db = scratch.connect().await;
    let tables: i64 = db
        .query_one_raw(statement(
            "SELECT COUNT(*) AS tables FROM pg_tables WHERE tablename = 'accounts'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "tables")
        .unwrap();
    assert_eq!(tables, 0);
    db.close().await.unwrap();
    holder.close().await.unwrap();
    scratch.drop_now().await;
}
