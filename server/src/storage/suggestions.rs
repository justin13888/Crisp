//! The ingredient-suggestion half of the repository layer
//! (`docs/ingredient-index.md` §6): filing a suggestion under a per-account
//! cap, the administrators' queue, triage with its audit row, and the read
//! the override export is built from.
//!
//! Triage and its `moderation_actions` row are written in one transaction,
//! and the rows written come back in the outcome, so the caller logs exactly
//! what was recorded.

use async_trait::async_trait;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
};

use super::PgStorage;
use super::moderation::{action, sql::audit, suggestion_target};
use crate::db::{account, ingredient_suggestion, moderation_action};

/// The `status` of a suggestion no administrator has triaged.
pub const OPEN: &str = "open";
/// The `status` of a suggestion an administrator accepted.
pub const ACCEPTED: &str = "accepted";

/// A suggestion to file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSuggestion {
    /// The suggesting account's `usr…` id.
    pub suggester: String,
    /// The suggestion kind, as stored.
    pub kind: String,
    /// The index entry it is about, if one.
    pub slug: Option<String>,
    /// The locale shard it is for.
    pub locale: String,
    /// The proposed text.
    pub text: String,
    /// The suggester's explanation.
    pub note: String,
    /// When it is filed (epoch seconds).
    pub at: i64,
}

/// What filing a suggestion did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSuggestion {
    /// The suggestion was stored.
    Filed(Box<ingredient_suggestion::Model>),
    /// The suggester already holds the most open suggestions allowed;
    /// nothing was stored.
    TooMany,
}

/// What triaging a suggestion did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveSuggestion {
    /// There is no suggestion with that id.
    Absent,
    /// The suggestion was already triaged; nothing changed.
    AlreadyResolved(ingredient_suggestion::Model),
    /// The suggestion is triaged, and the audit row written.
    Resolved {
        /// The suggestion as triaged.
        suggestion: ingredient_suggestion::Model,
        /// The audit row.
        audit: moderation_action::Model,
    },
}

/// The ingredient-suggestion operations of the repository layer.
// See the matching allowance on `Storage`.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait Suggestions: Send + Sync {
    /// Files `suggestion` unless its suggester already holds `max_open` open
    /// suggestions. The count and the insert happen under a lock on the
    /// suggester's account, so concurrent suggestions cannot exceed the cap.
    async fn file_suggestion(
        &self,
        suggestion: NewSuggestion,
        max_open: u64,
    ) -> Result<FileSuggestion, DbErr>;
    /// Lists suggestions, oldest first, optionally only those with `status`.
    async fn suggestions(
        &self,
        status: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<ingredient_suggestion::Model>, DbErr>;
    /// Lists at most `limit` accepted suggestions triaged at or after
    /// `since` (epoch seconds), in the order they were accepted.
    async fn accepted_suggestions(
        &self,
        since: i64,
        limit: u64,
    ) -> Result<Vec<ingredient_suggestion::Model>, DbErr>;
    /// Triages open suggestion `id` as `resolution` on behalf of `actor`,
    /// writing its `moderation_actions` row in the same transaction.
    async fn resolve_suggestion(
        &self,
        id: i64,
        resolution: &str,
        actor: &str,
        at: i64,
    ) -> Result<ResolveSuggestion, DbErr>;
}

#[async_trait]
impl Suggestions for PgStorage {
    #[tracing::instrument(level = "debug", skip_all, fields(suggester = %suggestion.suggester))]
    async fn file_suggestion(
        &self,
        suggestion: NewSuggestion,
        max_open: u64,
    ) -> Result<FileSuggestion, DbErr> {
        file(&self.db, suggestion, max_open).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn suggestions(
        &self,
        status: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<ingredient_suggestion::Model>, DbErr> {
        let mut select = ingredient_suggestion::Entity::find();
        if let Some(status) = status {
            select = select.filter(ingredient_suggestion::Column::Status.eq(status));
        }
        select
            .order_by_asc(ingredient_suggestion::Column::Id)
            .limit(limit)
            .offset(offset)
            .all(&self.db)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn accepted_suggestions(
        &self,
        since: i64,
        limit: u64,
    ) -> Result<Vec<ingredient_suggestion::Model>, DbErr> {
        ingredient_suggestion::Entity::find()
            .filter(ingredient_suggestion::Column::Status.eq(ACCEPTED))
            .filter(ingredient_suggestion::Column::ResolvedAt.gte(since))
            .order_by_asc(ingredient_suggestion::Column::ResolvedAt)
            .order_by_asc(ingredient_suggestion::Column::Id)
            .limit(limit)
            .all(&self.db)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn resolve_suggestion(
        &self,
        id: i64,
        resolution: &str,
        actor: &str,
        at: i64,
    ) -> Result<ResolveSuggestion, DbErr> {
        resolve(&self.db, id, resolution, actor, at).await
    }
}

/// Files `suggestion` under the suggester's account lock.
async fn file(
    db: &DatabaseConnection,
    suggestion: NewSuggestion,
    max_open: u64,
) -> Result<FileSuggestion, DbErr> {
    let txn = db.begin().await?;
    // Serializes one suggester's suggestions, so the count below stays true
    // until this transaction ends.
    account::Entity::find_by_id(suggestion.suggester.clone())
        .lock_exclusive()
        .one(&txn)
        .await?;
    let open = ingredient_suggestion::Entity::find()
        .filter(ingredient_suggestion::Column::Suggester.eq(suggestion.suggester.as_str()))
        .filter(ingredient_suggestion::Column::Status.eq(OPEN))
        .count(&txn)
        .await?;
    if open >= max_open {
        tracing::debug!(open, max_open, "suggestion refused: too many open");
        return Ok(FileSuggestion::TooMany);
    }
    let filed = ingredient_suggestion::ActiveModel {
        id: NotSet,
        suggester: Set(Some(suggestion.suggester)),
        kind: Set(suggestion.kind),
        slug: Set(suggestion.slug),
        locale: Set(suggestion.locale),
        text: Set(suggestion.text),
        note: Set(suggestion.note),
        status: Set(OPEN.to_owned()),
        created_at: Set(suggestion.at),
        resolved_at: Set(None),
        resolved_by: Set(None),
    }
    .insert(&txn)
    .await?;
    txn.commit().await?;
    Ok(FileSuggestion::Filed(Box::new(filed)))
}

/// Triages open suggestion `id` and writes its audit row, in one
/// transaction holding the suggestion's row lock.
async fn resolve(
    db: &DatabaseConnection,
    id: i64,
    resolution: &str,
    actor: &str,
    at: i64,
) -> Result<ResolveSuggestion, DbErr> {
    let txn = db.begin().await?;
    let Some(found) = ingredient_suggestion::Entity::find_by_id(id)
        .lock_exclusive()
        .one(&txn)
        .await?
    else {
        return Ok(ResolveSuggestion::Absent);
    };
    if found.status != OPEN {
        return Ok(ResolveSuggestion::AlreadyResolved(found));
    }
    let mut active: ingredient_suggestion::ActiveModel = found.into();
    active.status = Set(resolution.to_owned());
    active.resolved_at = Set(Some(at));
    active.resolved_by = Set(Some(actor.to_owned()));
    let suggestion = active.update(&txn).await?;
    let entry = audit(
        &txn,
        actor,
        action::RESOLVE_SUGGESTION,
        suggestion_target(id),
        Some(resolution),
        None,
        at,
    )
    .await?;
    txn.commit().await?;
    Ok(ResolveSuggestion::Resolved {
        suggestion,
        audit: entry,
    })
}
