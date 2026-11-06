//! The SQL behind [`super::Moderation`]. Each administrator action runs in
//! one transaction that also writes its audit rows; a refusal drops the
//! transaction, which rolls back.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DbErr,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
};

use super::{
    FileReport, Lift, NewReport, NewSuspension, NewTakedown, Resolve, Suspend, TakeDown, action,
    covering, document_target, report_target,
};
use crate::db::{
    account, bundle, challenge, moderation_action, report, session, suspension, takedown,
};

/// The `status` of a report no administrator has resolved.
const OPEN: &str = "open";

/// Appends one audit row through `conn`.
async fn audit(
    conn: &impl ConnectionTrait,
    actor: &str,
    action: &str,
    target: String,
    reason: Option<&str>,
    report_id: Option<i64>,
    at: i64,
) -> Result<moderation_action::Model, DbErr> {
    moderation_action::ActiveModel {
        id: NotSet,
        actor: Set(actor.to_owned()),
        action: Set(action.to_owned()),
        target: Set(target),
        reason: Set(reason.map(str::to_owned)),
        report_id: Set(report_id),
        created_at: Set(at),
    }
    .insert(conn)
    .await
}

/// The bundles of `doc_id`, narrowed to `revision` when one is given.
fn matching(doc_id: &str, revision: Option<&str>) -> Condition {
    let document = Condition::all().add(bundle::Column::DocId.eq(doc_id));
    match revision {
        Some(revision) => document.add(bundle::Column::Revision.eq(revision)),
        None => document,
    }
}

/// Deletes the bundles of `doc_id` (only `revision`, when given) and writes
/// the tombstone and its audit row through `conn`; `None` when no bundle
/// matched.
async fn remove(
    conn: &impl ConnectionTrait,
    takedown: &NewTakedown,
) -> Result<Option<(takedown::Model, moderation_action::Model)>, DbErr> {
    let revision = takedown.revision.as_deref();
    let hashes: Vec<String> = bundle::Entity::find()
        .filter(matching(&takedown.doc_id, revision))
        .select_only()
        .column(bundle::Column::ContentHash)
        .lock_exclusive()
        .into_tuple()
        .all(conn)
        .await?;
    if hashes.is_empty() {
        return Ok(None);
    }
    bundle::Entity::delete_many()
        .filter(matching(&takedown.doc_id, revision))
        .exec(conn)
        .await?;
    let tombstone = takedown::ActiveModel {
        id: NotSet,
        doc_id: Set(takedown.doc_id.clone()),
        revision: Set(takedown.revision.clone()),
        content_hashes: Set(serde_json::json!(hashes)),
        reason: Set(takedown.reason.clone()),
        report_id: Set(takedown.report_id),
        created_at: Set(takedown.at),
    }
    .insert(conn)
    .await?;
    let entry = audit(
        conn,
        &takedown.actor,
        action::TAKEDOWN,
        document_target(&takedown.doc_id, revision),
        Some(&takedown.reason),
        takedown.report_id,
        takedown.at,
    )
    .await?;
    tracing::debug!(removed = hashes.len(), "takedown removed bundles");
    Ok(Some((tombstone, entry)))
}

pub(super) async fn file_report(
    db: &DatabaseConnection,
    report: NewReport,
    max_open: u64,
) -> Result<FileReport, DbErr> {
    let txn = db.begin().await?;
    // Serializes one reporter's reports, so the count below stays true
    // until this transaction ends.
    account::Entity::find_by_id(report.reporter.clone())
        .lock_exclusive()
        .one(&txn)
        .await?;
    let open = report::Entity::find()
        .filter(report::Column::Reporter.eq(report.reporter.as_str()))
        .filter(report::Column::Status.eq(OPEN))
        .count(&txn)
        .await?;
    if open >= max_open {
        tracing::debug!(open, max_open, "report refused: too many open");
        return Ok(FileReport::TooMany);
    }
    let filed = report::ActiveModel {
        id: NotSet,
        reporter: Set(Some(report.reporter)),
        doc_id: Set(report.doc_id),
        revision: Set(report.revision),
        category: Set(report.category),
        detail: Set(report.detail),
        status: Set(OPEN.to_owned()),
        created_at: Set(report.at),
        resolved_at: Set(None),
        resolved_by: Set(None),
    }
    .insert(&txn)
    .await?;
    txn.commit().await?;
    Ok(FileReport::Filed(filed))
}

pub(super) async fn reports(
    db: &DatabaseConnection,
    status: Option<&str>,
    limit: u64,
    offset: u64,
) -> Result<Vec<report::Model>, DbErr> {
    let mut select = report::Entity::find();
    if let Some(status) = status {
        select = select.filter(report::Column::Status.eq(status));
    }
    select
        .order_by_asc(report::Column::Id)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await
}

pub(super) async fn report(
    db: &DatabaseConnection,
    id: i64,
) -> Result<Option<report::Model>, DbErr> {
    report::Entity::find_by_id(id).one(db).await
}

pub(super) async fn resolve_report(
    db: &DatabaseConnection,
    id: i64,
    resolution: &str,
    actor: &str,
    at: i64,
) -> Result<Resolve, DbErr> {
    let txn = db.begin().await?;
    let Some(found) = report::Entity::find_by_id(id)
        .lock_exclusive()
        .one(&txn)
        .await?
    else {
        return Ok(Resolve::Absent);
    };
    if found.status != OPEN {
        return Ok(Resolve::AlreadyResolved(found));
    }
    let mut active: report::ActiveModel = found.into();
    active.status = Set(resolution.to_owned());
    active.resolved_at = Set(Some(at));
    active.resolved_by = Set(Some(actor.to_owned()));
    let report = active.update(&txn).await?;
    let entry = audit(
        &txn,
        actor,
        action::RESOLVE_REPORT,
        report_target(id),
        Some(resolution),
        Some(id),
        at,
    )
    .await?;
    txn.commit().await?;
    Ok(Resolve::Resolved {
        report,
        audit: entry,
    })
}

pub(super) async fn take_down(
    db: &DatabaseConnection,
    takedown: NewTakedown,
) -> Result<TakeDown, DbErr> {
    let txn = db.begin().await?;
    let existing = tombstones(&txn, &takedown.doc_id).await?;
    if let Some(covered) = covering(&existing, takedown.revision.as_deref()) {
        return Ok(TakeDown::AlreadyRemoved(covered.clone()));
    }
    let Some((tombstone, audit)) = remove(&txn, &takedown).await? else {
        return Ok(TakeDown::Absent);
    };
    txn.commit().await?;
    Ok(TakeDown::Removed { tombstone, audit })
}

pub(super) async fn tombstones(
    conn: &impl ConnectionTrait,
    doc_id: &str,
) -> Result<Vec<takedown::Model>, DbErr> {
    takedown::Entity::find()
        .filter(takedown::Column::DocId.eq(doc_id))
        .order_by_asc(takedown::Column::Id)
        .all(conn)
        .await
}

pub(super) async fn lift_takedown(
    db: &DatabaseConnection,
    doc_id: &str,
    actor: &str,
    at: i64,
) -> Result<Lift, DbErr> {
    let txn = db.begin().await?;
    let lifted = takedown::Entity::delete_many()
        .filter(takedown::Column::DocId.eq(doc_id))
        .exec(&txn)
        .await?
        .rows_affected;
    if lifted == 0 {
        return Ok(Lift::Absent);
    }
    let entry = audit(
        &txn,
        actor,
        action::LIFT_TAKEDOWN,
        doc_id.to_owned(),
        None,
        None,
        at,
    )
    .await?;
    txn.commit().await?;
    Ok(Lift::Lifted {
        tombstones: lifted,
        audit: entry,
    })
}

pub(super) async fn suspension(
    db: &DatabaseConnection,
    usr_id: &str,
) -> Result<Option<suspension::Model>, DbErr> {
    suspension::Entity::find_by_id(usr_id.to_owned())
        .one(db)
        .await
}

pub(super) async fn suspend(
    db: &DatabaseConnection,
    request: NewSuspension,
) -> Result<Suspend, DbErr> {
    let txn = db.begin().await?;
    let usr_id = request.usr_id.as_str();
    // The same lock a publish takes, so nothing is published under the
    // account while it is being suspended.
    let locked = account::Entity::find_by_id(usr_id.to_owned())
        .lock_exclusive()
        .one(&txn)
        .await?;
    if locked.is_none() {
        return Ok(Suspend::Absent);
    }
    if suspension::Entity::find_by_id(usr_id.to_owned())
        .one(&txn)
        .await?
        .is_some()
    {
        return Ok(Suspend::AlreadySuspended);
    }
    suspension::ActiveModel {
        usr_id: Set(usr_id.to_owned()),
        reason: Set(request.reason.clone()),
        actor: Set(request.actor.clone()),
        created_at: Set(request.at),
    }
    .insert(&txn)
    .await?;
    let sessions = session::Entity::delete_many()
        .filter(session::Column::UsrId.eq(usr_id))
        .exec(&txn)
        .await?
        .rows_affected;
    challenge::Entity::delete_many()
        .filter(challenge::Column::UsrId.eq(usr_id))
        .exec(&txn)
        .await?;
    let mut takedowns = Vec::new();
    let mut entries = Vec::new();
    if request.take_down_all {
        let documents: Vec<String> = bundle::Entity::find()
            .filter(bundle::Column::PublishedBy.eq(usr_id))
            .select_only()
            .column(bundle::Column::DocId)
            .distinct()
            .order_by_asc(bundle::Column::DocId)
            .into_tuple()
            .all(&txn)
            .await?;
        for doc_id in documents {
            let takedown = NewTakedown {
                actor: request.actor.clone(),
                doc_id,
                revision: None,
                reason: request.reason.clone(),
                report_id: None,
                at: request.at,
            };
            if let Some((tombstone, entry)) = remove(&txn, &takedown).await? {
                takedowns.push(tombstone);
                entries.push(entry);
            }
        }
    }
    let entry = audit(
        &txn,
        &request.actor,
        action::SUSPEND,
        usr_id.to_owned(),
        Some(&request.reason),
        None,
        request.at,
    )
    .await?;
    entries.push(entry);
    txn.commit().await?;
    Ok(Suspend::Suspended {
        sessions,
        takedowns,
        audit: entries,
    })
}

pub(super) async fn unsuspend(
    db: &DatabaseConnection,
    usr_id: &str,
    actor: &str,
    at: i64,
) -> Result<Option<moderation_action::Model>, DbErr> {
    let txn = db.begin().await?;
    let lifted = suspension::Entity::delete_by_id(usr_id.to_owned())
        .exec(&txn)
        .await?
        .rows_affected;
    if lifted == 0 {
        return Ok(None);
    }
    let entry = audit(
        &txn,
        actor,
        action::UNSUSPEND,
        usr_id.to_owned(),
        None,
        None,
        at,
    )
    .await?;
    txn.commit().await?;
    Ok(Some(entry))
}
