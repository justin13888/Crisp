//! The SQL behind deletion: unpublishing a publisher's own revisions of a
//! document, and deleting an account with everything it published.
//!
//! Deletion is hard: rows are removed, never flagged. A bundle's index rows
//! go with it through the `recipes_index` foreign-key cascade, and an
//! account's sessions, challenges, and bundles go with it through the
//! cascades on `usr_id` and `published_by`.

use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect,
    TransactionTrait,
};

use super::quota;
use crate::db::{account, bundle};
use crate::quota::Usage;

/// What an unpublish request found and did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unpublish {
    /// No bundle matched the document (and revision); nothing was removed.
    Absent,
    /// Bundles matched, but another account published every one of them;
    /// nothing was removed.
    NotOwner,
    /// The caller's matching bundles were removed: how many, and their
    /// stored bytes. Matching bundles other accounts published remain.
    Removed(Usage),
}

/// The bundles of `doc_id`, narrowed to `revision` when one is given.
fn matching(doc_id: &str, revision: Option<&str>) -> Condition {
    let document = Condition::all().add(bundle::Column::DocId.eq(doc_id));
    match revision {
        Some(revision) => document.add(bundle::Column::Revision.eq(revision)),
        None => document,
    }
}

/// Decides what unpublishing means for the `(published_by, size_bytes)` of
/// every matching bundle: absent when there are none, refused when
/// `usr_id` published none, otherwise the count and bytes of its own.
#[must_use]
pub fn unpublish_outcome(rows: &[(String, i64)], usr_id: &str) -> Unpublish {
    if rows.is_empty() {
        return Unpublish::Absent;
    }
    let own = rows.iter().filter(|(by, _)| by == usr_id);
    let bundles = own.clone().count() as u64;
    if bundles == 0 {
        return Unpublish::NotOwner;
    }
    let bytes = own.map(|(_, size)| u64::try_from(*size).unwrap_or(0)).sum();
    Unpublish::Removed(Usage { bytes, bundles })
}

/// Removes the bundles of `doc_id` (only `revision`, when given) that
/// `usr_id` published, in one transaction.
///
/// The matching rows are locked first, so the outcome reported is exactly
/// what was removed even under a concurrent unpublish of the same rows.
pub(super) async fn unpublish(
    db: &DatabaseConnection,
    usr_id: &str,
    doc_id: &str,
    revision: Option<&str>,
) -> Result<Unpublish, DbErr> {
    let txn = db.begin().await?;
    let rows: Vec<(String, i64)> = bundle::Entity::find()
        .filter(matching(doc_id, revision))
        .select_only()
        .column(bundle::Column::PublishedBy)
        .column(bundle::Column::SizeBytes)
        .lock_exclusive()
        .into_tuple()
        .all(&txn)
        .await?;
    let outcome = unpublish_outcome(&rows, usr_id);
    tracing::debug!(matched = rows.len(), ?outcome, "unpublish decided");
    if let Unpublish::Removed(_) = outcome {
        bundle::Entity::delete_many()
            .filter(matching(doc_id, revision))
            .filter(bundle::Column::PublishedBy.eq(usr_id))
            .exec(&txn)
            .await?;
        txn.commit().await?;
    }
    Ok(outcome)
}

/// Deletes the `usr_id` account and, through the schema's cascades, its
/// sessions, login challenges, and every bundle it published, in one
/// transaction. Returns what it published, or `None` when there is no such
/// account.
///
/// The account row is locked first, the same lock a publish takes before
/// its quota check, so no bundle can be published under the account while
/// it is being deleted. The removed count and bytes are logged at INFO
/// before the transaction commits.
pub(super) async fn delete_account(
    db: &DatabaseConnection,
    usr_id: &str,
) -> Result<Option<Usage>, DbErr> {
    let txn = db.begin().await?;
    let locked = account::Entity::find_by_id(usr_id.to_owned())
        .lock_exclusive()
        .one(&txn)
        .await?;
    if locked.is_none() {
        tracing::debug!(usr_id, "account to delete is already gone");
        return Ok(None);
    }
    let removed = quota::usage(&txn, usr_id).await?;
    tracing::info!(
        usr_id,
        bundles = removed.bundles,
        bytes = removed.bytes,
        "deleting account and every bundle it published"
    );
    account::Entity::delete_by_id(usr_id.to_owned())
        .exec(&txn)
        .await?;
    txn.commit().await?;
    Ok(Some(removed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(by: &str, size: i64) -> (String, i64) {
        (by.to_owned(), size)
    }

    #[test]
    fn nothing_matched_is_absent() {
        assert_eq!(unpublish_outcome(&[], "usr1a"), Unpublish::Absent);
    }

    #[test]
    fn only_another_accounts_revisions_is_not_owner() {
        let rows = [row("usr1b", 10), row("usr1c", 20)];
        assert_eq!(unpublish_outcome(&rows, "usr1a"), Unpublish::NotOwner);
    }

    #[test]
    fn removed_counts_only_the_callers_own_revisions() {
        let rows = [row("usr1a", 10), row("usr1b", 20), row("usr1a", 5)];
        assert_eq!(
            unpublish_outcome(&rows, "usr1a"),
            Unpublish::Removed(Usage {
                bytes: 15,
                bundles: 2
            })
        );
    }
}
