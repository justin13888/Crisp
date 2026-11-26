//! The `rate_limits` table: the abuse-limit counters every replica shares
//! (see [`crate::limits::PgLimiter`]).
//!
//! One row per `(operation, bucket_key)` holds the key's GCRA theoretical
//! arrival time in microseconds of the database clock. A row whose arrival
//! time has passed is indistinguishable from a fresh key, so the purge task
//! deletes it, which keeps the table bounded by the keys active within one
//! period. The table is read and written only by raw SQL, so it has no
//! entity.

pub use migration::RateLimits;

/// The migration creating `rate_limits`.
mod migration {
    use sea_orm_migration::prelude::*;
    use sea_orm_migration::sea_orm::ConnectionTrait;

    /// Creates `rate_limits` and the index the purge deletes through. It
    /// only adds a table, so the previous release runs unchanged against the
    /// migrated schema during a rolling update.
    pub struct RateLimits;

    impl MigrationName for RateLimits {
        fn name(&self) -> &str {
            "m20261126_000008_rate_limits"
        }
    }

    #[async_trait::async_trait]
    impl MigrationTrait for RateLimits {
        async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
            manager.get_connection().execute_unprepared(UP).await?;
            Ok(())
        }

        async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
            manager.get_connection().execute_unprepared(DOWN).await?;
            Ok(())
        }
    }

    /// The forward migration.
    const UP: &str = "
        CREATE TABLE rate_limits (
            operation   TEXT NOT NULL,
            bucket_key  TEXT NOT NULL,
            tat         BIGINT NOT NULL,
            PRIMARY KEY (operation, bucket_key)
        );
        CREATE INDEX rate_limits_tat_idx ON rate_limits (tat);
    ";

    /// The reverse migration.
    const DOWN: &str = "
        DROP TABLE IF EXISTS rate_limits;
    ";
}
