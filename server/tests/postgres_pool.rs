//! PostgreSQL-backed tests of the connection pool settings, gated on
//! `CRISP_TEST_DATABASE_URL` exactly as `tests/postgres.rs` is: skipped
//! locally when it is unset, failing where `CI` is set without it.
//!
//! They check that every pooled connection carries the configured
//! `statement_timeout`, that the readiness ping reaches the database, and
//! that a statement the timeout cancels (SQLSTATE `57014`) and a refused
//! connection are both classified as unavailability (503), not as a fault.

use std::time::Duration;

use crisp_server::config::DatabaseOptions;
use crisp_server::storage::{PgStorage, Storage, is_unavailable};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

/// Connects to the test database under `options`, or `None` to skip.
///
/// # Panics
///
/// When `CRISP_TEST_DATABASE_URL` is unset but `CI` is set.
async fn connect(test: &str, options: &DatabaseOptions) -> Option<DatabaseConnection> {
    let Ok(url) = std::env::var("CRISP_TEST_DATABASE_URL") else {
        assert!(
            std::env::var_os("CI").is_none(),
            "{test}: CI is set but CRISP_TEST_DATABASE_URL is not; \
             the PostgreSQL tests must run in CI"
        );
        eprintln!("{test} skipped: set CRISP_TEST_DATABASE_URL to run it");
        return None;
    };
    let db = sea_orm::Database::connect(options.connect_options(&url))
        .await
        .expect("connect");
    Some(db)
}

/// Pool settings with a short statement timeout.
fn short_timeout() -> DatabaseOptions {
    DatabaseOptions {
        max_connections: 2,
        acquire_timeout: Duration::from_secs(5),
        statement_timeout: Duration::from_millis(250),
    }
}

/// `sql` as a raw PostgreSQL statement.
fn statement(sql: &str) -> Statement {
    Statement::from_string(DbBackend::Postgres, sql)
}

#[tokio::test]
async fn pooled_connections_carry_the_statement_timeout() {
    let Some(db) = connect("statement_timeout", &short_timeout()).await else {
        return;
    };
    let row = db
        .query_one(statement("SHOW statement_timeout"))
        .await
        .expect("show")
        .expect("a row");
    let value: String = row.try_get("", "statement_timeout").expect("a setting");
    assert_eq!(value, "250ms");
}

#[tokio::test]
async fn the_readiness_ping_reaches_the_database() {
    let Some(db) = connect("ping", &short_timeout()).await else {
        return;
    };
    PgStorage::new(db).ping().await.expect("ping");
}

#[tokio::test]
async fn the_readiness_ping_fails_when_the_database_is_unreachable() {
    let mut options = DatabaseOptions {
        acquire_timeout: Duration::from_secs(2),
        ..short_timeout()
    }
    .connect_options("postgresql://crisp@127.0.0.1:1/crisp");
    // A lazy pool opens no connection until the ping asks for one, so the
    // pool builds and the refused connect surfaces through the ping itself.
    options.connect_lazy(true);
    let db = sea_orm::Database::connect(options)
        .await
        .expect("a lazy pool connects without a server");
    let error = PgStorage::new(db)
        .ping()
        .await
        .expect_err("nothing listens on port 1");
    assert!(is_unavailable(&error), "{error} must be unavailable");
}

#[tokio::test]
async fn a_statement_cut_off_by_the_timeout_is_unavailable() {
    let Some(db) = connect("query_canceled", &short_timeout()).await else {
        return;
    };
    let error = db
        .execute(statement("SELECT pg_sleep(5)"))
        .await
        .expect_err("the statement timeout cancels it");
    assert!(is_unavailable(&error), "{error} must be unavailable");
    // A statement that is merely wrong is not.
    let error = db
        .execute(statement("SELECT * FROM no_such_table"))
        .await
        .expect_err("an undefined table");
    assert!(!is_unavailable(&error), "{error} must not be unavailable");
}

#[tokio::test]
async fn a_refused_connection_is_unavailable() {
    let options = DatabaseOptions {
        acquire_timeout: Duration::from_secs(2),
        ..short_timeout()
    };
    // Port 1 is reserved and nothing listens on it, so the connect is refused.
    let error =
        sea_orm::Database::connect(options.connect_options("postgresql://crisp@127.0.0.1:1/crisp"))
            .await
            .expect_err("nothing listens on port 1");
    assert!(is_unavailable(&error), "{error} must be unavailable");
}
