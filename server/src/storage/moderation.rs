//! The moderation half of the repository layer: reports, takedowns and
//! their tombstones, suspensions, and the audit trail they write
//! (`docs/public-server-policy.md` §4).
//!
//! Every administrator action and its `moderation_actions` row are written in
//! one transaction, and the rows written come back in the outcome, so the
//! caller logs exactly what was recorded.

use async_trait::async_trait;
use sea_orm::DbErr;

use super::PgStorage;
use crate::db::{moderation_action, report, revocation, suspension, takedown};

mod sql;

/// The audit-trail `action` values.
pub mod action {
    /// A document or revision was taken down.
    pub const TAKEDOWN: &str = "takedown";
    /// A document's takedowns were lifted.
    pub const LIFT_TAKEDOWN: &str = "lift_takedown";
    /// An account was suspended.
    pub const SUSPEND: &str = "suspend";
    /// An account's suspension was lifted.
    pub const UNSUSPEND: &str = "unsuspend";
    /// A report was closed.
    pub const RESOLVE_REPORT: &str = "resolve_report";
}

/// A report to file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReport {
    /// The reporting account's `usr…` id.
    pub reporter: String,
    /// The reported document id.
    pub doc_id: String,
    /// The reported revision, if one.
    pub revision: Option<String>,
    /// The report category.
    pub category: String,
    /// The reporter's explanation.
    pub detail: String,
    /// When it is filed (epoch seconds).
    pub at: i64,
}

/// What filing a report did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileReport {
    /// The report was stored.
    Filed(report::Model),
    /// The reporter already holds the most open reports allowed; nothing was
    /// stored.
    TooMany,
}

/// What resolving a report did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolve {
    /// There is no report with that id.
    Absent,
    /// The report was already resolved; nothing changed.
    AlreadyResolved(report::Model),
    /// The report is resolved, and the audit row written.
    Resolved {
        /// The report as resolved.
        report: report::Model,
        /// The audit row.
        audit: moderation_action::Model,
    },
}

/// A takedown to carry out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTakedown {
    /// The administrator's `usr…` id.
    pub actor: String,
    /// The document to remove.
    pub doc_id: String,
    /// The one revision to remove; `None` removes every revision.
    pub revision: Option<String>,
    /// The reason category.
    pub reason: String,
    /// The report that prompted it, if any.
    pub report_id: Option<i64>,
    /// When it happens (epoch seconds).
    pub at: i64,
}

/// What a takedown did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TakeDown {
    /// No bundle matched; nothing was removed.
    Absent,
    /// A tombstone already covers the target; nothing was removed.
    AlreadyRemoved(takedown::Model),
    /// The matching bundles were deleted and the tombstone written.
    Removed {
        /// The tombstone.
        tombstone: takedown::Model,
        /// The audit row.
        audit: moderation_action::Model,
    },
}

/// What lifting a document's takedowns did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lift {
    /// The document has no tombstone.
    Absent,
    /// Every tombstone of the document was deleted.
    Lifted {
        /// How many tombstones were deleted.
        tombstones: u64,
        /// The audit row.
        audit: moderation_action::Model,
    },
}

/// A suspension to carry out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSuspension {
    /// The administrator's `usr…` id.
    pub actor: String,
    /// The account to suspend.
    pub usr_id: String,
    /// The reason category.
    pub reason: String,
    /// Whether every document the account published is taken down too.
    pub take_down_all: bool,
    /// When it happens (epoch seconds).
    pub at: i64,
}

/// What a suspension did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suspend {
    /// There is no such account.
    Absent,
    /// The account is already suspended; nothing changed.
    AlreadySuspended,
    /// The account is suspended and its sessions and challenges deleted.
    Suspended {
        /// How many sessions were ended.
        sessions: u64,
        /// The tombstones of the documents taken down with it.
        takedowns: Vec<takedown::Model>,
        /// The audit rows: one per takedown, then the suspension's.
        audit: Vec<moderation_action::Model>,
    },
}

/// The tombstone among `tombstones` (one document's) that covers
/// `revision`: one for the whole document, or, when `revision` is given, one
/// for that revision.
#[must_use]
pub fn covering<'a>(
    tombstones: &'a [takedown::Model],
    revision: Option<&str>,
) -> Option<&'a takedown::Model> {
    tombstones
        .iter()
        .find(|t| t.revision.is_none() || (revision.is_some() && t.revision.as_deref() == revision))
}

/// The audit `target` naming `doc_id`, or one `revision` of it.
#[must_use]
pub fn document_target(doc_id: &str, revision: Option<&str>) -> String {
    match revision {
        Some(revision) => format!("{doc_id}/{revision}"),
        None => doc_id.to_owned(),
    }
}

/// The audit `target` naming report `id`.
#[must_use]
pub fn report_target(id: i64) -> String {
    format!("report/{id}")
}

/// The moderation operations of the repository layer.
// See the matching allowance on `Storage`.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait Moderation: Send + Sync {
    /// Files `report` unless its reporter already holds `max_open` open
    /// reports. The count and the insert happen under a lock on the
    /// reporter's account, so concurrent reports cannot exceed the cap.
    async fn file_report(&self, report: NewReport, max_open: u64) -> Result<FileReport, DbErr>;
    /// Lists reports, oldest first, optionally only those with `status`.
    async fn reports(
        &self,
        status: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<report::Model>, DbErr>;
    /// Fetches report `id`.
    async fn report(&self, id: i64) -> Result<Option<report::Model>, DbErr>;
    /// Closes open report `id` as `resolution` on behalf of `actor`.
    async fn resolve_report(
        &self,
        id: i64,
        resolution: &str,
        actor: &str,
        at: i64,
    ) -> Result<Resolve, DbErr>;
    /// Deletes the bundles `takedown` names, whoever published them,
    /// revokes their endorsements, and leaves a tombstone, in one
    /// transaction.
    async fn take_down(&self, takedown: NewTakedown) -> Result<TakeDown, DbErr>;
    /// Every endorsement this server revoked, oldest first (spec §8.8).
    async fn revocations(&self) -> Result<Vec<revocation::Model>, DbErr>;
    /// Whether this server revoked its endorsement of the bundle whose hex
    /// `content_hash` is given.
    async fn is_revoked(&self, content_hash: &str) -> Result<bool, DbErr>;
    /// The tombstones of `doc_id`, oldest first.
    async fn tombstones(&self, doc_id: &str) -> Result<Vec<takedown::Model>, DbErr>;
    /// Deletes every tombstone of `doc_id` on behalf of `actor`.
    async fn lift_takedown(&self, doc_id: &str, actor: &str, at: i64) -> Result<Lift, DbErr>;
    /// The suspension of `usr_id`, if it is suspended.
    async fn suspension(&self, usr_id: &str) -> Result<Option<suspension::Model>, DbErr>;
    /// Suspends an account, ends its sessions and challenges, and, when
    /// asked, takes every document it published down, in one transaction.
    async fn suspend(&self, suspension: NewSuspension) -> Result<Suspend, DbErr>;
    /// Lifts the suspension of `usr_id` on behalf of `actor`; returns the
    /// audit row, or `None` when it was not suspended.
    async fn unsuspend(
        &self,
        usr_id: &str,
        actor: &str,
        at: i64,
    ) -> Result<Option<moderation_action::Model>, DbErr>;
}

#[async_trait]
impl Moderation for PgStorage {
    #[tracing::instrument(level = "debug", skip_all, fields(reporter = %report.reporter))]
    async fn file_report(&self, report: NewReport, max_open: u64) -> Result<FileReport, DbErr> {
        sql::file_report(&self.db, report, max_open).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn reports(
        &self,
        status: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<report::Model>, DbErr> {
        sql::reports(&self.db, status, limit, offset).await
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn report(&self, id: i64) -> Result<Option<report::Model>, DbErr> {
        sql::report(&self.db, id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn resolve_report(
        &self,
        id: i64,
        resolution: &str,
        actor: &str,
        at: i64,
    ) -> Result<Resolve, DbErr> {
        sql::resolve_report(&self.db, id, resolution, actor, at).await
    }

    #[tracing::instrument(level = "debug", skip_all, fields(doc_id = %takedown.doc_id))]
    async fn take_down(&self, takedown: NewTakedown) -> Result<TakeDown, DbErr> {
        sql::take_down(&self.db, takedown).await
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn revocations(&self) -> Result<Vec<revocation::Model>, DbErr> {
        sql::revocations(&self.db).await
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn is_revoked(&self, content_hash: &str) -> Result<bool, DbErr> {
        sql::is_revoked(&self.db, content_hash).await
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn tombstones(&self, doc_id: &str) -> Result<Vec<takedown::Model>, DbErr> {
        sql::tombstones(&self.db, doc_id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn lift_takedown(&self, doc_id: &str, actor: &str, at: i64) -> Result<Lift, DbErr> {
        sql::lift_takedown(&self.db, doc_id, actor, at).await
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn suspension(&self, usr_id: &str) -> Result<Option<suspension::Model>, DbErr> {
        sql::suspension(&self.db, usr_id).await
    }

    #[tracing::instrument(level = "debug", skip_all, fields(usr_id = %suspension.usr_id))]
    async fn suspend(&self, suspension: NewSuspension) -> Result<Suspend, DbErr> {
        sql::suspend(&self.db, suspension).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn unsuspend(
        &self,
        usr_id: &str,
        actor: &str,
        at: i64,
    ) -> Result<Option<moderation_action::Model>, DbErr> {
        sql::unsuspend(&self.db, usr_id, actor, at).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tombstone(revision: Option<&str>) -> takedown::Model {
        takedown::Model {
            id: 1,
            doc_id: "rcp1a".to_owned(),
            revision: revision.map(str::to_owned),
            content_hashes: serde_json::json!([]),
            reason: "spam".to_owned(),
            report_id: None,
            created_at: 1,
        }
    }

    #[test]
    fn a_document_tombstone_covers_every_revision() {
        let tombstones = [tombstone(None)];
        assert!(covering(&tombstones, None).is_some());
        assert!(covering(&tombstones, Some("rev1a")).is_some());
    }

    #[test]
    fn a_revision_tombstone_covers_only_that_revision() {
        let tombstones = [tombstone(Some("rev1a"))];
        assert!(covering(&tombstones, Some("rev1a")).is_some());
        assert!(covering(&tombstones, Some("rev1b")).is_none());
        assert!(covering(&tombstones, None).is_none());
        assert!(covering(&[], None).is_none());
    }

    #[test]
    fn targets_name_documents_revisions_and_reports() {
        assert_eq!(document_target("rcp1a", None), "rcp1a");
        assert_eq!(document_target("rcp1a", Some("rev1a")), "rcp1a/rev1a");
        assert_eq!(report_target(7), "report/7");
    }
}
