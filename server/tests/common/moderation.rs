//! [`MockStorage`]'s moderation half: reports, tombstones, suspensions, and
//! the audit rows each administrator action appends, over the same vectors.

use async_trait::async_trait;
use crisp_server::db::{moderation_action, report, revocation, suspension, takedown};
use crisp_server::storage::{
    FileReport, Lift, Moderation, NewReport, NewSuspension, NewTakedown, Resolve, Suspend,
    TakeDown, action, covering, document_target, report_target,
};
use sea_orm::DbErr;

use super::mock::{Inner, MockStorage};

/// Appends one audit row to `inner` and returns it.
fn audit(
    inner: &mut Inner,
    actor: &str,
    action: &str,
    target: String,
    reason: Option<&str>,
    report_id: Option<i64>,
    at: i64,
) -> moderation_action::Model {
    inner.next_id += 1;
    let entry = moderation_action::Model {
        id: inner.next_id,
        actor: actor.to_owned(),
        action: action.to_owned(),
        target,
        reason: reason.map(str::to_owned),
        report_id,
        created_at: at,
    };
    inner.actions.push(entry.clone());
    entry
}

/// Removes what `takedown` names and writes its tombstone and audit row, as
/// the SQL does; `None` when no bundle matched.
fn remove(
    inner: &mut Inner,
    takedown: &NewTakedown,
) -> Option<(takedown::Model, moderation_action::Model)> {
    let revision = takedown.revision.as_deref();
    let hashes = MockStorage::remove_bundles(inner, |b| {
        b.doc_id == takedown.doc_id && revision.is_none_or(|r| b.revision == r)
    });
    if hashes.is_empty() {
        return None;
    }
    for hash in &hashes {
        if !inner.revocations.iter().any(|r| &r.content_hash == hash) {
            inner.revocations.push(revocation::Model {
                content_hash: hash.clone(),
                revoked_at: takedown.at,
            });
        }
    }
    inner.next_id += 1;
    let tombstone = takedown::Model {
        id: inner.next_id,
        doc_id: takedown.doc_id.clone(),
        revision: takedown.revision.clone(),
        content_hashes: serde_json::json!(hashes),
        reason: takedown.reason.clone(),
        report_id: takedown.report_id,
        created_at: takedown.at,
    };
    inner.takedowns.push(tombstone.clone());
    let entry = audit(
        inner,
        &takedown.actor,
        action::TAKEDOWN,
        document_target(&takedown.doc_id, revision),
        Some(&takedown.reason),
        takedown.report_id,
        takedown.at,
    );
    Some((tombstone, entry))
}

/// The tombstones of `doc_id` in `inner`, oldest first.
fn tombstones_of(inner: &Inner, doc_id: &str) -> Vec<takedown::Model> {
    (inner.takedowns.iter().filter(|t| t.doc_id == doc_id))
        .cloned()
        .collect()
}

#[async_trait]
impl Moderation for MockStorage {
    async fn file_report(&self, report: NewReport, max_open: u64) -> Result<FileReport, DbErr> {
        self.check("file_report")?;
        let mut inner = self.inner.lock().unwrap();
        let open = (inner.reports.iter())
            .filter(|r| r.reporter.as_deref() == Some(&report.reporter) && r.status == "open")
            .count() as u64;
        if open >= max_open {
            return Ok(FileReport::TooMany);
        }
        inner.next_id += 1;
        let row = report::Model {
            id: inner.next_id,
            reporter: Some(report.reporter),
            doc_id: report.doc_id,
            revision: report.revision,
            category: report.category,
            detail: report.detail,
            status: "open".to_owned(),
            created_at: report.at,
            resolved_at: None,
            resolved_by: None,
        };
        inner.reports.push(row.clone());
        Ok(FileReport::Filed(row))
    }

    async fn reports(
        &self,
        status: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<report::Model>, DbErr> {
        self.check("reports")?;
        let inner = self.inner.lock().unwrap();
        Ok((inner.reports.iter())
            .filter(|r| status.is_none_or(|s| r.status == s))
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn report(&self, id: i64) -> Result<Option<report::Model>, DbErr> {
        self.check("report")?;
        let inner = self.inner.lock().unwrap();
        Ok(inner.reports.iter().find(|r| r.id == id).cloned())
    }

    async fn resolve_report(
        &self,
        id: i64,
        resolution: &str,
        actor: &str,
        at: i64,
    ) -> Result<Resolve, DbErr> {
        self.check("resolve_report")?;
        let mut inner = self.inner.lock().unwrap();
        let Some(row) = inner.reports.iter_mut().find(|r| r.id == id) else {
            return Ok(Resolve::Absent);
        };
        if row.status != "open" {
            return Ok(Resolve::AlreadyResolved(row.clone()));
        }
        row.status = resolution.to_owned();
        row.resolved_at = Some(at);
        row.resolved_by = Some(actor.to_owned());
        let report = row.clone();
        let target = report_target(id);
        let audit = audit(
            &mut inner,
            actor,
            action::RESOLVE_REPORT,
            target,
            Some(resolution),
            Some(id),
            at,
        );
        Ok(Resolve::Resolved { report, audit })
    }

    async fn take_down(&self, takedown: NewTakedown) -> Result<TakeDown, DbErr> {
        self.check("take_down")?;
        let mut inner = self.inner.lock().unwrap();
        let existing = tombstones_of(&inner, &takedown.doc_id);
        if let Some(covered) = covering(&existing, takedown.revision.as_deref()) {
            return Ok(TakeDown::AlreadyRemoved(covered.clone()));
        }
        Ok(match remove(&mut inner, &takedown) {
            Some((tombstone, audit)) => TakeDown::Removed { tombstone, audit },
            None => TakeDown::Absent,
        })
    }

    async fn revocations(&self) -> Result<Vec<revocation::Model>, DbErr> {
        self.check("revocations")?;
        let mut revoked = self.inner.lock().unwrap().revocations.clone();
        revoked
            .sort_by(|a, b| (a.revoked_at, &a.content_hash).cmp(&(b.revoked_at, &b.content_hash)));
        Ok(revoked)
    }

    async fn is_revoked(&self, content_hash: &str) -> Result<bool, DbErr> {
        self.check("is_revoked")?;
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .revocations
            .iter()
            .any(|r| r.content_hash == content_hash))
    }

    async fn tombstones(&self, doc_id: &str) -> Result<Vec<takedown::Model>, DbErr> {
        self.check("tombstones")?;
        Ok(tombstones_of(&self.inner.lock().unwrap(), doc_id))
    }

    async fn lift_takedown(&self, doc_id: &str, actor: &str, at: i64) -> Result<Lift, DbErr> {
        self.check("lift_takedown")?;
        let mut inner = self.inner.lock().unwrap();
        let before = inner.takedowns.len();
        inner.takedowns.retain(|t| t.doc_id != doc_id);
        let tombstones = (before - inner.takedowns.len()) as u64;
        if tombstones == 0 {
            return Ok(Lift::Absent);
        }
        let target = doc_id.to_owned();
        let audit = audit(
            &mut inner,
            actor,
            action::LIFT_TAKEDOWN,
            target,
            None,
            None,
            at,
        );
        Ok(Lift::Lifted { tombstones, audit })
    }

    async fn suspension(&self, usr_id: &str) -> Result<Option<suspension::Model>, DbErr> {
        self.check("suspension")?;
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .suspensions
            .iter()
            .find(|s| s.usr_id == usr_id)
            .cloned())
    }

    async fn suspend(&self, request: NewSuspension) -> Result<Suspend, DbErr> {
        self.check("suspend")?;
        let mut inner = self.inner.lock().unwrap();
        let usr_id = request.usr_id.as_str();
        if !inner.accounts.iter().any(|a| a.usr_id == usr_id) {
            return Ok(Suspend::Absent);
        }
        if inner.suspensions.iter().any(|s| s.usr_id == usr_id) {
            return Ok(Suspend::AlreadySuspended);
        }
        inner.suspensions.push(suspension::Model {
            usr_id: usr_id.to_owned(),
            reason: request.reason.clone(),
            actor: request.actor.clone(),
            created_at: request.at,
        });
        let before = inner.sessions.len();
        inner.sessions.retain(|s| s.usr_id != usr_id);
        let sessions = (before - inner.sessions.len()) as u64;
        inner.challenges.retain(|c| c.usr_id != usr_id);
        let (mut takedowns, mut entries) = (Vec::new(), Vec::new());
        if request.take_down_all {
            let mut documents: Vec<String> = (inner.bundles.iter())
                .filter(|b| b.published_by == usr_id)
                .map(|b| b.doc_id.clone())
                .collect();
            documents.sort();
            documents.dedup();
            for doc_id in documents {
                let takedown = NewTakedown {
                    actor: request.actor.clone(),
                    doc_id,
                    revision: None,
                    reason: request.reason.clone(),
                    report_id: None,
                    at: request.at,
                };
                if let Some((tombstone, entry)) = remove(&mut inner, &takedown) {
                    takedowns.push(tombstone);
                    entries.push(entry);
                }
            }
        }
        let target = usr_id.to_owned();
        let (actor, reason) = (&request.actor, Some(request.reason.as_str()));
        entries.push(audit(
            &mut inner,
            actor,
            action::SUSPEND,
            target,
            reason,
            None,
            request.at,
        ));
        Ok(Suspend::Suspended {
            sessions,
            takedowns,
            audit: entries,
        })
    }

    async fn unsuspend(
        &self,
        usr_id: &str,
        actor: &str,
        at: i64,
    ) -> Result<Option<moderation_action::Model>, DbErr> {
        self.check("unsuspend")?;
        let mut inner = self.inner.lock().unwrap();
        let before = inner.suspensions.len();
        inner.suspensions.retain(|s| s.usr_id != usr_id);
        if inner.suspensions.len() == before {
            return Ok(None);
        }
        let target = usr_id.to_owned();
        Ok(Some(audit(
            &mut inner,
            actor,
            action::UNSUSPEND,
            target,
            None,
            None,
            at,
        )))
    }
}
