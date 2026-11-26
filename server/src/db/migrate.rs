//! Applying the migrations safely when several replicas start at once.
//!
//! Before [`Migrator::up`] runs, the server takes the session-level advisory
//! lock [`MIGRATION_LOCK`] on a dedicated one-connection pool and holds it
//! until the migrations finish. Replicas starting together serialise on it:
//! the first applies what is pending, and each one after finds nothing left
//! to apply. The lock is taken with `pg_try_advisory_lock`, retried every
//! [`LOCK_RETRY`], so no statement blocks past `statement_timeout`, and a
//! wait longer than the configured timeout fails the start instead of
//! hanging. A replica that dies holding the lock releases it with its
//! connection.

use std::time::{Duration, Instant};

use sea_orm::{ConnectOptions, ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};
use sea_orm_migration::MigratorTrait;

use super::Migrator;

/// The advisory-lock key every replica's migration run takes (the bytes of
/// `crisp_mg`). Any session may inspect it in `pg_locks` (`classid` and
/// `objid` hold its high and low halves).
pub const MIGRATION_LOCK: i64 = 0x6372_6973_705f_6d67;

/// How often a replica waiting for [`MIGRATION_LOCK`] asks for it again.
pub const LOCK_RETRY: Duration = Duration::from_millis(250);

/// Why a migration run failed.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// Another session held [`MIGRATION_LOCK`] for longer than the timeout.
    #[error(
        "timed out after {waited:?} waiting for the migration lock ({MIGRATION_LOCK:#x}); \
         another replica is migrating or a session holds the lock"
    )]
    LockTimeout {
        /// How long this replica waited.
        waited: Duration,
    },
    /// The database refused a statement or a connection.
    #[error(transparent)]
    Db(#[from] DbErr),
}

/// What a migration run did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrationReport {
    /// How long this replica waited for [`MIGRATION_LOCK`].
    pub waited: Duration,
    /// How many migrations this replica applied (zero when another replica
    /// had already applied them).
    pub applied: usize,
}

/// Applies every pending migration under [`MIGRATION_LOCK`], connecting
/// with `options` narrowed to one connection, and waiting at most
/// `lock_timeout` for the lock.
///
/// # Errors
///
/// [`MigrateError::LockTimeout`] if the lock stays held past `lock_timeout`;
/// [`MigrateError::Db`] if connecting, locking, or a migration fails.
#[tracing::instrument(level = "info", skip_all, fields(lock = MIGRATION_LOCK))]
pub async fn migrate(
    mut options: ConnectOptions,
    lock_timeout: Duration,
) -> Result<MigrationReport, MigrateError> {
    // One connection, so every statement below, the migrations' included,
    // runs in the session that holds the lock.
    options.max_connections(1).min_connections(1);
    let conn = sea_orm::Database::connect(options).await?;
    let outcome = locked(&conn, lock_timeout).await;
    if let Err(error) = conn.close().await {
        tracing::warn!(%error, "closing the migration connection failed");
    }
    outcome
}

/// Takes the lock on `conn`, migrates, and releases it.
async fn locked(
    conn: &DatabaseConnection,
    lock_timeout: Duration,
) -> Result<MigrationReport, MigrateError> {
    let waited = acquire(conn, lock_timeout).await?;
    let applied = apply(conn).await;
    let released = release(conn).await;
    let applied = applied?;
    released?;
    tracing::info!(
        lock = MIGRATION_LOCK,
        waited_ms = waited.as_millis(),
        applied,
        "migrations applied; migration lock released"
    );
    Ok(MigrationReport { waited, applied })
}

/// Applies the pending migrations, returning how many there were.
async fn apply(conn: &DatabaseConnection) -> Result<usize, DbErr> {
    let pending = Migrator::get_pending_migrations(conn).await?.len();
    tracing::info!(pending, "applying pending migrations");
    Migrator::up(conn, None).await?;
    Ok(pending)
}

/// Waits for [`MIGRATION_LOCK`] on `conn`, returning how long it took.
async fn acquire(
    conn: &DatabaseConnection,
    lock_timeout: Duration,
) -> Result<Duration, MigrateError> {
    let started = Instant::now();
    let mut logged = false;
    loop {
        if try_lock(conn).await? {
            let waited = started.elapsed();
            tracing::info!(
                lock = MIGRATION_LOCK,
                waited_ms = waited.as_millis(),
                "migration lock acquired"
            );
            return Ok(waited);
        }
        let waited = started.elapsed();
        if waited >= lock_timeout {
            tracing::error!(
                lock = MIGRATION_LOCK,
                waited_ms = waited.as_millis(),
                "timed out waiting for the migration lock"
            );
            return Err(MigrateError::LockTimeout { waited });
        }
        if !logged {
            tracing::info!(
                lock = MIGRATION_LOCK,
                timeout_secs = lock_timeout.as_secs(),
                "another replica holds the migration lock; waiting"
            );
            logged = true;
        }
        tokio::time::sleep(LOCK_RETRY.min(lock_timeout.saturating_sub(waited))).await;
    }
}

/// One `pg_try_advisory_lock` attempt.
async fn try_lock(conn: &DatabaseConnection) -> Result<bool, DbErr> {
    boolean(conn, "SELECT pg_try_advisory_lock($1) AS held").await
}

/// Releases [`MIGRATION_LOCK`]. A lock this session no longer held (its
/// connection was replaced mid-run) is logged, since another replica may
/// then have migrated alongside it.
async fn release(conn: &DatabaseConnection) -> Result<(), DbErr> {
    if !boolean(conn, "SELECT pg_advisory_unlock($1) AS held").await? {
        tracing::warn!(
            lock = MIGRATION_LOCK,
            "the migration lock was not held at release"
        );
    }
    Ok(())
}

/// Runs `sql`, which takes [`MIGRATION_LOCK`] as `$1` and returns one
/// boolean column `held`.
async fn boolean(conn: &DatabaseConnection, sql: &str) -> Result<bool, DbErr> {
    let row = conn
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [MIGRATION_LOCK.into()],
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("an advisory-lock call returned no row".to_owned()))?;
    row.try_get("", "held")
}
