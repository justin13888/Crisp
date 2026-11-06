//! The SQL behind storage quotas: locking a publisher and measuring what it
//! and the whole server store. [`Quota::check`] makes the decision.

use sea_orm::{ConnectionTrait, DbBackend, DbErr, Statement, Value};

use super::BundleInsertError;
use crate::db::bundle;
use crate::quota::{Quota, Usage};

/// The transaction-scoped advisory lock every insert takes while a
/// server-wide cap is set, so two publishers cannot both fit the last room.
/// The value is arbitrary; it only has to be the same for every insert.
const SERVER_CAP_LOCK: i64 = 0x6372_6973_7071_7561;

/// A Postgres statement with positional `values`.
fn statement(sql: &str, values: impl IntoIterator<Item = Value>) -> Statement {
    Statement::from_sql_and_values(DbBackend::Postgres, sql, values)
}

/// A SQL `BIGINT` count or sum as `u64`; negative values cannot occur and
/// read as zero.
fn unsigned(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

/// The bytes and bundles `usr_id` has published, read through `conn`.
pub(super) async fn usage(conn: &impl ConnectionTrait, usr_id: &str) -> Result<Usage, DbErr> {
    let row = conn
        .query_one_raw(statement(
            "SELECT COALESCE(SUM(size_bytes), 0)::BIGINT AS bytes, COUNT(*) AS bundles \
             FROM bundles WHERE published_by = $1",
            [usr_id.into()],
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("an aggregate returned no row".to_owned()))?;
    Ok(Usage {
        bytes: unsigned(row.try_get("", "bytes")?),
        bundles: unsigned(row.try_get("", "bundles")?),
    })
}

/// The bytes every account stores together.
async fn server_total(conn: &impl ConnectionTrait) -> Result<u64, DbErr> {
    let row = conn
        .query_one_raw(statement(
            "SELECT COALESCE(SUM(size_bytes), 0)::BIGINT AS bytes FROM bundles",
            [],
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("an aggregate returned no row".to_owned()))?;
    Ok(unsigned(row.try_get("", "bytes")?))
}

/// Admits `row` against `quota` inside the insert transaction `txn`.
///
/// Locks the publisher's `accounts` row first, so a second insert by the
/// same account waits here until this transaction ends and then measures
/// usage that includes this bundle. The per-account quotas are checked
/// before the server-wide lock is taken, so an account over its own quota
/// never waits on other publishers.
///
/// # Errors
///
/// [`BundleInsertError::Quota`] when the bundle does not fit; a database
/// error otherwise.
pub(super) async fn admit(
    txn: &impl ConnectionTrait,
    quota: &Quota,
    row: &bundle::ActiveModel,
) -> Result<(), BundleInsertError> {
    let (Some(usr_id), Some(size)) = (row.published_by.try_as_ref(), row.size_bytes.try_as_ref())
    else {
        return Err(
            DbErr::Custom("a bundle row needs published_by and size_bytes".to_owned()).into(),
        );
    };
    let size = unsigned(*size);
    txn.execute_raw(statement(
        "SELECT 1 FROM accounts WHERE usr_id = $1 FOR UPDATE",
        [usr_id.as_str().into()],
    ))
    .await?;
    let account = usage(txn, usr_id).await?;
    tracing::debug!(
        used_bytes = account.bytes,
        used_bundles = account.bundles,
        size,
        "checking account quota"
    );
    quota.check(account, None, size)?;
    if quota.max_stored_bytes.is_some() {
        txn.execute_raw(statement(
            "SELECT pg_advisory_xact_lock($1)",
            [SERVER_CAP_LOCK.into()],
        ))
        .await?;
        let total = server_total(txn).await?;
        tracing::debug!(total, size, "checking server storage cap");
        quota.check(account, Some(total), size)?;
    }
    Ok(())
}
