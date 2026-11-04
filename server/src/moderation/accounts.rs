//! Suspension (`docs/public-server-policy.md` §4.4): an administrator blocks
//! an account's key.
//!
//! A suspended account's sessions and login challenges are deleted at
//! suspension, it cannot sign in (403 `account_suspended`), and its key
//! cannot register again. The suspension is keyed by the `usr…` id, not tied
//! to the account row, so it outlives the account's deletion.

use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Category, log_action};
use crate::auth::{AccountPath, AdminSession};
use crate::error::{Conflict, NotFound};
use crate::operation_error;
use crate::state::Store;
use crate::storage::{NewSuspension, Suspend};
use crate::util::now_epoch;

/// The `POST /admin/accounts/{usr_id}/suspend` request body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct SuspendRequest {
    /// Why the account is suspended.
    pub reason: Category,
    /// Whether to take down, in the same action, every document the account
    /// published (default `false`).
    pub take_down_all: Option<bool>,
}

/// What a suspension did.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct SuspensionView {
    /// The suspended account.
    pub usr_id: String,
    /// Why it was suspended.
    pub reason: Category,
    /// How many of its sessions were ended.
    pub sessions_ended: u64,
    /// The documents taken down with it.
    pub taken_down: Vec<String>,
}

operation_error! {
    /// What `POST /admin/accounts/{usr_id}/suspend` can fail with.
    pub enum SuspendError { NotFound, Conflict }
}

operation_error! {
    /// What `POST /admin/accounts/{usr_id}/unsuspend` can fail with.
    pub enum UnsuspendError { NotFound }
}

/// Suspends an account, ends its sessions, and, with `take_down_all`, takes
/// down every document it published (each with its own tombstone and audit
/// row).
///
/// Refused 404 for an unknown account and 409 for one already suspended.
#[kynos::post("/admin/accounts/{usr_id}/suspend")]
#[tracing::instrument(level = "info", skip_all, fields(usr_id = %path.usr_id))]
pub async fn suspend(
    admin: AdminSession,
    Inject(storage): Inject<Store>,
    Path(path): Path<AccountPath>,
    Json(request): Json<SuspendRequest>,
) -> Result<Json<SuspensionView>, SuspendError> {
    let caller = admin.into_inner().resolve()?;
    let suspension = NewSuspension {
        actor: caller.account.usr_id,
        usr_id: path.usr_id.clone(),
        reason: request.reason.as_str().to_owned(),
        take_down_all: request.take_down_all.unwrap_or(false),
        at: now_epoch(),
    };
    match storage.suspend(suspension).await? {
        Suspend::Suspended {
            sessions,
            takedowns,
            audit,
        } => {
            audit.iter().for_each(log_action);
            Ok(Json(SuspensionView {
                usr_id: path.usr_id,
                reason: request.reason,
                sessions_ended: sessions,
                taken_down: takedowns.into_iter().map(|t| t.doc_id).collect(),
            }))
        }
        Suspend::Absent => Err(NotFound::new("no account with that id").into()),
        Suspend::AlreadySuspended => Err(Conflict::new("the account is already suspended").into()),
    }
}

/// Lifts an account's suspension: it may sign in again. Documents taken
/// down with it stay down until each takedown is lifted.
///
/// Refused 404 when the account is not suspended.
#[kynos::post("/admin/accounts/{usr_id}/unsuspend")]
#[tracing::instrument(level = "info", skip_all, fields(usr_id = %path.usr_id))]
pub async fn unsuspend(
    admin: AdminSession,
    Inject(storage): Inject<Store>,
    Path(path): Path<AccountPath>,
) -> Result<NoContent, UnsuspendError> {
    let caller = admin.into_inner().resolve()?;
    let lifted = storage
        .unsuspend(&path.usr_id, &caller.account.usr_id, now_epoch())
        .await?;
    match lifted {
        Some(audit) => {
            log_action(&audit);
            Ok(NoContent)
        }
        None => Err(NotFound::new("that account is not suspended").into()),
    }
}
