//! The PostgreSQL [`Limiter`]: counters in the `rate_limits` table
//! (`db/rate_limits.rs`), shared by every replica.
//!
//! Each spend is one statement, so it costs one round trip and is atomic: the
//! row's theoretical arrival time is read and advanced under the row lock
//! `INSERT ... ON CONFLICT DO UPDATE` takes, so two replicas spending from
//! one key serialise on that row and can never both take the last unit. Time
//! is the database's clock (`clock_timestamp()`), so replicas whose clocks
//! drift still agree.

use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};

use super::{Budget, Decision, Limiter, Operation};

/// The database clock in microseconds since the Unix epoch, as SQL.
const NOW_MICROS: &str = "(EXTRACT(EPOCH FROM clock_timestamp()) * 1000000)::BIGINT";

/// Spends one unit for `($1, $2)` with refill interval `$3` and tolerance
/// `$4` (microseconds), the [`super::gcra`] step in SQL: a fresh key is
/// inserted spent once; an existing one is advanced only while its arrival
/// time is within the tolerance of the clock, otherwise left untouched.
/// `allowed` says which; on a refusal `tat` is the key's arrival time, from
/// which the wait follows (it can trail a concurrent spend by one unit,
/// which only shortens the `Retry-After`).
fn spend_sql() -> String {
    format!(
        "WITH clock AS (SELECT {NOW_MICROS} AS now), \
         spent AS ( \
             INSERT INTO rate_limits AS r (operation, bucket_key, tat) \
             SELECT $1, $2, clock.now + $3 FROM clock \
             ON CONFLICT (operation, bucket_key) DO UPDATE \
                 SET tat = GREATEST(r.tat, EXCLUDED.tat - $3) + $3 \
                 WHERE GREATEST(r.tat, EXCLUDED.tat - $3) - (EXCLUDED.tat - $3) <= $4 \
             RETURNING r.tat \
         ) \
         SELECT EXISTS (SELECT 1 FROM spent) AS allowed, clock.now AS now, \
             (SELECT tat FROM rate_limits WHERE operation = $1 AND bucket_key = $2) AS tat \
         FROM clock"
    )
}

/// Rate-limit counters in PostgreSQL.
#[derive(Debug, Clone)]
pub struct PgLimiter {
    /// The connection pool the counters are read and written through.
    db: DatabaseConnection,
}

impl PgLimiter {
    /// Counters in the `rate_limits` table reached through `db`.
    #[must_use]
    pub const fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

/// The microseconds a refused key waits: until its arrival time is back
/// within the tolerance, or one refill interval when the row was not visible
/// to the statement's snapshot.
fn refused_wait(budget: Budget, now: i64, tat: Option<i64>) -> i64 {
    tat.map_or(budget.interval_micros(), |tat| {
        tat.saturating_sub(now)
            .saturating_sub(budget.tolerance_micros())
            .max(0)
    })
}

// See `Limiter`: the `#[must_use]` is `async_trait`'s.
#[allow(clippy::double_must_use)]
#[async_trait]
impl Limiter for PgLimiter {
    #[tracing::instrument(level = "trace", skip(self, budget), fields(operation = operation.name()))]
    async fn spend(
        &self,
        operation: Operation,
        key: &str,
        budget: Budget,
    ) -> Result<Decision, DbErr> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                spend_sql(),
                [
                    operation.name().into(),
                    key.into(),
                    budget.interval_micros().into(),
                    budget.tolerance_micros().into(),
                ],
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("the rate-limit spend returned no row".to_owned()))?;
        let allowed: bool = row.try_get("", "allowed")?;
        if allowed {
            return Ok(Decision::Allowed);
        }
        let now: i64 = row.try_get("", "now")?;
        let tat: Option<i64> = row.try_get("", "tat")?;
        Ok(Decision::from_gcra::<()>(Err(refused_wait(
            budget, now, tat,
        ))))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn purge(&self) -> Result<u64, DbErr> {
        let deleted = self
            .db
            .execute_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("DELETE FROM rate_limits WHERE tat <= {NOW_MICROS}"),
            ))
            .await?
            .rows_affected();
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_refused_wait_runs_until_the_arrival_time_is_within_tolerance() {
        // 3 per minute: a unit every 20 s, a tolerance of 40 s.
        let budget = Budget::new(NonZeroU32::new(3).unwrap(), Duration::from_secs(60));
        let second = 1_000_000;
        assert_eq!(refused_wait(budget, 0, Some(60 * second)), 20 * second);
        assert_eq!(refused_wait(budget, 0, Some(10 * second)), 0);
        assert_eq!(refused_wait(budget, 0, None), 20 * second);
    }

    #[test]
    fn the_spend_statement_runs_on_the_database_clock() {
        let sql = spend_sql();
        assert!(sql.contains("clock_timestamp()"), "{sql}");
        assert!(sql.contains("ON CONFLICT (operation, bucket_key) DO UPDATE"));
    }
}
