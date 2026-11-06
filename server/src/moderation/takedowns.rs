//! Takedowns (`docs/public-server-policy.md` §4.3): an administrator removes
//! a document, or one revision, whoever published it.
//!
//! Unlike a publisher's own removal, a takedown leaves a tombstone — the
//! `doc_id`, the removed `content_hash` values, the reason, and the time —
//! so reads answer 410 `removed` and a publish of the document answers 409
//! `removed` until the takedown is lifted.

use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Category, log_action};
use crate::auth::AdminSession;
use crate::bundles::DocPath;
use crate::db::takedown;
use crate::error::{Conflict, Internal, NotFound};
use crate::operation_error;
use crate::state::Store;
use crate::storage::{Lift, NewTakedown, TakeDown};
use crate::util::now_epoch;

/// The `POST /admin/takedowns` request body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct TakedownRequest {
    /// The document to remove.
    pub doc_id: String,
    /// The one revision to remove; every revision when absent.
    pub revision: Option<String>,
    /// Why it is removed.
    pub reason: Category,
    /// The report that prompted the takedown, if any.
    pub report_id: Option<i64>,
}

/// A takedown's tombstone.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct TakedownView {
    /// The removed document id.
    pub doc_id: String,
    /// The removed revision, or `null` when the whole document was removed.
    pub revision: Option<String>,
    /// The content hash of every bundle removed.
    pub content_hashes: Vec<String>,
    /// Why it was removed.
    pub reason: Category,
    /// The report that prompted it, if any.
    pub report_id: Option<i64>,
    /// When it was removed (epoch seconds).
    pub created_at: i64,
}

impl TryFrom<takedown::Model> for TakedownView {
    type Error = Internal;

    fn try_from(row: takedown::Model) -> Result<Self, Internal> {
        Ok(Self {
            content_hashes: serde_json::from_value(row.content_hashes)
                .map_err(|e| Internal::logged("takedowns.content_hashes", &e))?,
            reason: Category::stored(&row.reason)?,
            doc_id: row.doc_id,
            revision: row.revision,
            report_id: row.report_id,
            created_at: row.created_at,
        })
    }
}

operation_error! {
    /// What `POST /admin/takedowns` can fail with.
    pub enum TakedownError { NotFound, Conflict }
}

operation_error! {
    /// What `DELETE /admin/takedowns/{doc_id}` can fail with.
    pub enum LiftTakedownError { NotFound }
}

/// Removes a document, or one revision, whoever published it, and leaves a
/// tombstone.
///
/// Refused 404 when nothing is published there or the named report does
/// not exist, and 409 when a tombstone already covers the target.
#[kynos::post("/admin/takedowns")]
#[tracing::instrument(level = "info", skip_all, fields(doc_id = %request.doc_id, revision = request.revision))]
pub async fn take_down(
    admin: AdminSession,
    Inject(storage): Inject<Store>,
    Json(request): Json<TakedownRequest>,
) -> Result<Json<TakedownView>, TakedownError> {
    let caller = admin.into_inner().resolve()?;
    if let Some(report_id) = request.report_id
        && storage.report(report_id).await?.is_none()
    {
        return Err(NotFound::new("no report with that id").into());
    }
    let takedown = NewTakedown {
        actor: caller.account.usr_id,
        doc_id: request.doc_id,
        revision: request.revision,
        reason: request.reason.as_str().to_owned(),
        report_id: request.report_id,
        at: now_epoch(),
    };
    match storage.take_down(takedown).await? {
        TakeDown::Removed { tombstone, audit } => {
            log_action(&audit);
            Ok(Json(tombstone.try_into()?))
        }
        TakeDown::Absent => {
            Err(NotFound::new("no bundle with that document id (and revision)").into())
        }
        TakeDown::AlreadyRemoved(tombstone) => {
            Err(Conflict::new(format!("this is already taken down ({})", tombstone.reason)).into())
        }
    }
}

/// Lifts every takedown of a document (for example after a counter-notice)
/// by deleting its tombstones; its authors may then publish it again. The
/// removed bundles are not restored.
///
/// Refused 404 when the document has no tombstone.
#[kynos::delete("/admin/takedowns/{doc_id}")]
#[tracing::instrument(level = "info", skip_all, fields(doc_id = %path.doc_id))]
pub async fn lift_takedown(
    admin: AdminSession,
    Inject(storage): Inject<Store>,
    Path(path): Path<DocPath>,
) -> Result<NoContent, LiftTakedownError> {
    let caller = admin.into_inner().resolve()?;
    let outcome = storage
        .lift_takedown(&path.doc_id, &caller.account.usr_id, now_epoch())
        .await?;
    match outcome {
        Lift::Lifted { tombstones, audit } => {
            tracing::debug!(tombstones, "takedowns lifted");
            log_action(&audit);
            Ok(NoContent)
        }
        Lift::Absent => Err(NotFound::new("that document has no takedown").into()),
    }
}
