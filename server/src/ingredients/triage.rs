//! The administrators' side of ingredient suggestions: the queue, triage,
//! and the export of accepted suggestions as override patches.
//!
//! Every route takes an [`AdminSession`]. Triage writes its
//! `moderation_actions` row in the transaction that performs it, and logs it
//! through [`log_action`]; listing and exporting change nothing.

use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use super::export::{SuggestionExport, export};
use super::suggestions::{SuggestionStatus, SuggestionView};
use crate::auth::AdminSession;
use crate::error::{Conflict, Internal, NotFound};
use crate::moderation::log_action;
use crate::operation_error;
use crate::search::{DEFAULT_LIMIT, MAX_LIMIT, MAX_OFFSET};
use crate::state::Store;
use crate::storage::ResolveSuggestion;
use crate::util::now_epoch;

/// The most accepted suggestions one export renders.
pub const MAX_EXPORT: u64 = 1000;

/// How an administrator triages a suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Triage {
    /// Accepted: it joins the override export.
    Accepted,
    /// Declined.
    Declined,
    /// It repeats another suggestion or what the index already holds.
    Duplicate,
}

impl From<Triage> for SuggestionStatus {
    fn from(triage: Triage) -> Self {
        match triage {
            Triage::Accepted => Self::Accepted,
            Triage::Declined => Self::Declined,
            Triage::Duplicate => Self::Duplicate,
        }
    }
}

/// The `GET /admin/ingredients/suggestions` response body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct SuggestionList {
    /// The suggestions, oldest first.
    pub suggestions: Vec<SuggestionView>,
}

/// The filter and pagination of `GET /admin/ingredients/suggestions`.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct SuggestionQuery {
    /// Only suggestions with this status; every suggestion when absent.
    pub status: Option<SuggestionStatus>,
    /// How many suggestions to return (default 50).
    #[schema(minimum = 1, maximum = MAX_LIMIT)]
    pub limit: Option<u64>,
    /// How many suggestions to skip (default 0, at most 10 000).
    #[schema(maximum = MAX_OFFSET)]
    pub offset: Option<u64>,
}

/// The filter of `GET /admin/ingredients/suggestions/export`.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct ExportQuery {
    /// Only suggestions accepted at or after this time (epoch seconds);
    /// every accepted suggestion when absent.
    pub since: Option<i64>,
}

/// What `POST /admin/ingredients/suggestions/{id}/resolve` captures.
#[derive(Debug, Schema, PathParams)]
pub struct SuggestionPath {
    /// The suggestion id.
    pub id: i64,
}

/// The `POST /admin/ingredients/suggestions/{id}/resolve` request body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct TriageRequest {
    /// How the suggestion is triaged.
    pub resolution: Triage,
}

operation_error! {
    /// What `GET /admin/ingredients/suggestions` can fail with.
    pub enum ListSuggestionsError {}
}

operation_error! {
    /// What `POST /admin/ingredients/suggestions/{id}/resolve` can fail with.
    pub enum ResolveSuggestionError { NotFound, Conflict }
}

operation_error! {
    /// What `GET /admin/ingredients/suggestions/export` can fail with.
    pub enum ExportSuggestionsError {}
}

/// Lists ingredient suggestions for administrators, oldest first,
/// optionally only those with one status (`?status=open` is the queue).
#[kynos::get("/admin/ingredients/suggestions")]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn list_suggestions(
    admin: AdminSession,
    Inject(storage): Inject<Store>,
    Query(query): Query<SuggestionQuery>,
) -> Result<Json<SuggestionList>, ListSuggestionsError> {
    let caller = admin.into_inner().resolve()?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let status = query.status.map(SuggestionStatus::as_str);
    let rows = storage
        .suggestions(status, limit, query.offset.unwrap_or(0))
        .await?;
    tracing::debug!(
        actor = %caller.account.usr_id,
        ?status,
        rows = rows.len(),
        "listed ingredient suggestions"
    );
    let suggestions = rows
        .into_iter()
        .map(SuggestionView::try_from)
        .collect::<Result<_, _>>()?;
    Ok(Json(SuggestionList { suggestions }))
}

/// Triages an open suggestion as `accepted`, `declined`, or `duplicate`.
///
/// Refused 404 for an unknown suggestion and 409 for one already triaged.
#[kynos::post("/admin/ingredients/suggestions/{id}/resolve")]
#[tracing::instrument(level = "info", skip_all, fields(suggestion_id = path.id))]
pub async fn resolve_suggestion(
    admin: AdminSession,
    Inject(storage): Inject<Store>,
    Path(path): Path<SuggestionPath>,
    Json(request): Json<TriageRequest>,
) -> Result<Json<SuggestionView>, ResolveSuggestionError> {
    let caller = admin.into_inner().resolve()?;
    let resolution = SuggestionStatus::from(request.resolution).as_str();
    let outcome = storage
        .resolve_suggestion(path.id, resolution, &caller.account.usr_id, now_epoch())
        .await?;
    match outcome {
        ResolveSuggestion::Resolved { suggestion, audit } => {
            log_action(&audit);
            Ok(Json(suggestion.try_into()?))
        }
        ResolveSuggestion::Absent => Err(NotFound::new("no suggestion with that id").into()),
        ResolveSuggestion::AlreadyResolved(suggestion) => {
            Err(Conflict::new(format!("the suggestion is already {}", suggestion.status)).into())
        }
    }
}

/// Exports accepted suggestions as patches to
/// `tools/ingredients/rules/overrides/`, plus notes for what a maintainer
/// must decide by hand, for an administrator to open as a pull request
/// upstream (`docs/ingredient-index.md` §6).
///
/// Renders at most 1000 suggestions, in acceptance order; when `complete`
/// is `false`, call again with `since` set to `last_accepted_at`.
#[kynos::get("/admin/ingredients/suggestions/export")]
#[tracing::instrument(level = "info", skip_all, fields(since = query.since))]
pub async fn export_suggestions(
    admin: AdminSession,
    Inject(storage): Inject<Store>,
    Query(query): Query<ExportQuery>,
) -> Result<Json<SuggestionExport>, ExportSuggestionsError> {
    let caller = admin.into_inner().resolve()?;
    let rows = storage
        .accepted_suggestions(query.since.unwrap_or(i64::MIN), MAX_EXPORT)
        .await?;
    let index = crisp_core::ingredients::index()
        .map_err(|e| Internal::logged("loading the ingredient index", &e))?;
    let complete = (rows.len() as u64) < MAX_EXPORT;
    let exported = export(&rows, index, complete)?;
    tracing::info!(
        actor = %caller.account.usr_id,
        suggestions = exported.suggestions.len(),
        complete,
        "exported accepted ingredient suggestions"
    );
    Ok(Json(exported))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_triage_maps_onto_its_status() {
        assert_eq!(
            SuggestionStatus::from(Triage::Accepted),
            SuggestionStatus::Accepted
        );
        assert_eq!(
            SuggestionStatus::from(Triage::Declined),
            SuggestionStatus::Declined
        );
        assert_eq!(
            SuggestionStatus::from(Triage::Duplicate),
            SuggestionStatus::Duplicate
        );
    }
}
