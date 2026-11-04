//! The repository layer: a mockable [`Storage`] trait and its PostgreSQL
//! implementation [`PgStorage`].
//!
//! Every database access goes through [`Storage`], so handlers depend on the
//! trait, not on SeaORM. Tests substitute an in-memory implementation;
//! production uses [`PgStorage`]. Bundle publication writes the bundle and
//! its index rows inside one transaction, so a partially-indexed bundle can
//! never be observed.

use async_trait::async_trait;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, TransactionTrait,
};
use serde_json::json;

use crate::db::{account, bundle, challenge, recipe_index, session};
use crate::quota::{Quota, Usage};

mod deletion;
mod error;
mod moderation;
mod quota;
mod sessions;

pub use deletion::{Unpublish, unpublish_outcome};
pub use error::{BundleInsertError, InsertError, is_unavailable};
pub use moderation::{
    FileReport, Lift, Moderation, NewReport, NewSuspension, NewTakedown, Resolve, Suspend,
    TakeDown, action, covering, document_target, report_target,
};
pub use sessions::session_digest;

/// How many expired rows one purge pass removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PurgeCounts {
    /// Expired sessions deleted.
    pub sessions: u64,
    /// Expired login challenges deleted.
    pub challenges: u64,
}

/// A full-text and faceted recipe search request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    /// Free-text query, matched against the `tsvector`.
    pub text: Option<String>,
    /// Required cuisine tags (all must be present).
    pub cuisine: Vec<String>,
    /// Required course classifications.
    pub course: Vec<String>,
    /// Required diet classifications.
    pub diet: Vec<String>,
    /// Allergens that must be *absent* from a match.
    pub allergen_free: Vec<String>,
    /// An exact identity-language filter.
    pub language: Option<String>,
    /// The maximum number of rows to return.
    pub limit: u64,
    /// The row offset for pagination.
    pub offset: u64,
}

/// The repository abstraction over persistent storage. The moderation
/// operations are its [`Moderation`] supertrait.
// `async_trait` marks each boxed-future method `#[must_use]`, which clippy then
// flags as redundant on a `Result`-returning method; the attribute is not ours.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait Storage: Moderation + Send + Sync + 'static {
    /// Checks that the backing store is reachable and answering, for the
    /// readiness probe. An in-memory implementation has nothing to lose a
    /// connection to, so the default reports it ready.
    async fn ping(&self) -> Result<(), DbErr> {
        Ok(())
    }

    /// Inserts a new account.
    ///
    /// Returns [`InsertError::Duplicate`] if the `usr…` id or username is
    /// already taken.
    async fn create_account(&self, account: account::ActiveModel) -> Result<(), InsertError>;
    /// Looks an account up by username.
    async fn account_by_username(&self, username: &str) -> Result<Option<account::Model>, DbErr>;
    /// Looks an account up by its `usr…` id.
    async fn account_by_usr_id(&self, usr_id: &str) -> Result<Option<account::Model>, DbErr>;

    /// Stores a pending login challenge.
    async fn create_challenge(&self, challenge: challenge::ActiveModel) -> Result<(), DbErr>;
    /// Atomically fetches and consumes a login challenge by its nonce: of any
    /// number of concurrent callers, at most one receives the row.
    async fn take_challenge(&self, nonce: &str) -> Result<Option<challenge::Model>, DbErr>;

    /// Stores a freshly issued session. `session.token` carries the bearer
    /// token as issued; a persistent implementation stores only its
    /// [`session_digest`], never the token itself.
    async fn create_session(&self, session: session::ActiveModel) -> Result<(), DbErr>;
    /// Fetches a session by the bearer token as presented, only if it has
    /// not expired and its account is not suspended — so a session a login
    /// issued while a suspension committed never authenticates.
    async fn valid_session(&self, token: &str) -> Result<Option<session::Model>, DbErr>;
    /// Deletes the session for the bearer token as presented, ending it.
    async fn delete_session(&self, token: &str) -> Result<(), DbErr>;
    /// Deletes every session `usr_id` holds, ending each one; returns how
    /// many were deleted.
    async fn delete_sessions_for(&self, usr_id: &str) -> Result<u64, DbErr>;
    /// Lists the unexpired sessions `usr_id` holds, newest first.
    async fn sessions_for(&self, usr_id: &str) -> Result<Vec<session::Model>, DbErr>;
    /// Deletes every session and challenge that expired at or before `now`.
    async fn purge_expired(&self, now: i64) -> Result<PurgeCounts, DbErr>;

    /// Inserts a bundle and its index rows in a single transaction, once
    /// [`Quota::check`] allows it.
    ///
    /// The publisher's quota is checked inside that transaction while its
    /// `accounts` row is locked, so concurrent inserts by one account are
    /// checked one after another and cannot exceed a quota together. When a
    /// server-wide cap is set, every insert is serialized for the check.
    ///
    /// Returns [`BundleInsertError::Quota`] if the bundle does not fit, and
    /// [`InsertError::Duplicate`] if the content hash or the
    /// `(doc_id, revision)` pair is already published.
    async fn insert_bundle(
        &self,
        bundle: bundle::ActiveModel,
        index_rows: Vec<recipe_index::ActiveModel>,
        quota: &Quota,
    ) -> Result<(), BundleInsertError>;
    /// Sums the bytes and counts the bundles `usr_id` has published.
    async fn account_usage(&self, usr_id: &str) -> Result<Usage, DbErr>;
    /// Removes the bundles of `doc_id` (only `revision`, when given) that
    /// `usr_id` published, in one transaction, leaving any other account's.
    ///
    /// Returns [`Unpublish::Absent`] when nothing matches and
    /// [`Unpublish::NotOwner`] when only other accounts' bundles match;
    /// neither removes anything.
    async fn unpublish(
        &self,
        usr_id: &str,
        doc_id: &str,
        revision: Option<&str>,
    ) -> Result<Unpublish, DbErr>;
    /// Deletes the `usr_id` account with its sessions, login challenges, and
    /// every bundle it published, in one transaction; returns what it had
    /// published, or `None` when there is no such account.
    async fn delete_account(&self, usr_id: &str) -> Result<Option<Usage>, DbErr>;
    /// Fetches the most recently published bundle for a document id.
    async fn bundle_latest(&self, doc_id: &str) -> Result<Option<bundle::Model>, DbErr>;
    /// Fetches the exact `(doc_id, revision)` bundle.
    async fn bundle_exact(
        &self,
        doc_id: &str,
        revision: &str,
    ) -> Result<Option<bundle::Model>, DbErr>;

    /// Lists indexed recipes, newest first, for paginated browsing.
    async fn browse(&self, limit: u64, offset: u64) -> Result<Vec<recipe_index::Model>, DbErr>;
    /// Runs a full-text and faceted recipe search.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<recipe_index::Model>, DbErr>;
}

/// The PostgreSQL-backed [`Storage`] implementation.
pub struct PgStorage {
    /// The SeaORM connection pool.
    db: DatabaseConnection,
}

impl PgStorage {
    /// Wraps an open database connection.
    #[must_use]
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

/// Builds the JSONB array literal used with the PostgreSQL `@>` containment
/// operator for a single faceted value.
fn contains(value: &str) -> serde_json::Value {
    json!([value])
}

#[async_trait]
impl Storage for PgStorage {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn ping(&self) -> Result<(), DbErr> {
        self.db.ping().await
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_account(&self, account: account::ActiveModel) -> Result<(), InsertError> {
        account
            .insert(&self.db)
            .await
            .map_err(InsertError::classify)?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn account_by_username(&self, username: &str) -> Result<Option<account::Model>, DbErr> {
        account::Entity::find()
            .filter(account::Column::Username.eq(username))
            .one(&self.db)
            .await
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn account_by_usr_id(&self, usr_id: &str) -> Result<Option<account::Model>, DbErr> {
        account::Entity::find_by_id(usr_id.to_owned())
            .one(&self.db)
            .await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn create_challenge(&self, challenge: challenge::ActiveModel) -> Result<(), DbErr> {
        challenge.insert(&self.db).await?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn take_challenge(&self, nonce: &str) -> Result<Option<challenge::Model>, DbErr> {
        // One `DELETE … WHERE nonce = $1 RETURNING …` statement: the row is
        // consumed and returned atomically, so a nonce can be redeemed once.
        let taken = challenge::Entity::delete_many()
            .filter(challenge::Column::Nonce.eq(nonce))
            .exec_with_returning(&self.db)
            .await?;
        Ok(taken.into_iter().next())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn create_session(&self, session: session::ActiveModel) -> Result<(), DbErr> {
        sessions::create(&self.db, session).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn valid_session(&self, token: &str) -> Result<Option<session::Model>, DbErr> {
        sessions::valid(&self.db, token).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn delete_session(&self, token: &str) -> Result<(), DbErr> {
        sessions::delete(&self.db, token).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn delete_sessions_for(&self, usr_id: &str) -> Result<u64, DbErr> {
        sessions::delete_for(&self.db, usr_id).await
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn sessions_for(&self, usr_id: &str) -> Result<Vec<session::Model>, DbErr> {
        sessions::live_for(&self.db, usr_id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn purge_expired(&self, now: i64) -> Result<PurgeCounts, DbErr> {
        let sessions = session::Entity::delete_many()
            .filter(session::Column::ExpiresAt.lte(now))
            .exec(&self.db)
            .await?
            .rows_affected;
        let challenges = challenge::Entity::delete_many()
            .filter(challenge::Column::ExpiresAt.lte(now))
            .exec(&self.db)
            .await?
            .rows_affected;
        Ok(PurgeCounts {
            sessions,
            challenges,
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn insert_bundle(
        &self,
        bundle: bundle::ActiveModel,
        index_rows: Vec<recipe_index::ActiveModel>,
        quota: &Quota,
    ) -> Result<(), BundleInsertError> {
        let txn = self.db.begin().await?;
        // A refusal drops `txn`, which rolls back and releases the locks.
        quota::admit(&txn, quota, &bundle).await?;
        bundle.insert(&txn).await.map_err(InsertError::classify)?;
        for row in index_rows {
            row.insert(&txn).await?;
        }
        txn.commit().await?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn account_usage(&self, usr_id: &str) -> Result<Usage, DbErr> {
        quota::usage(&self.db, usr_id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn unpublish(
        &self,
        usr_id: &str,
        doc_id: &str,
        revision: Option<&str>,
    ) -> Result<Unpublish, DbErr> {
        deletion::unpublish(&self.db, usr_id, doc_id, revision).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn delete_account(&self, usr_id: &str) -> Result<Option<Usage>, DbErr> {
        deletion::delete_account(&self.db, usr_id).await
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn bundle_latest(&self, doc_id: &str) -> Result<Option<bundle::Model>, DbErr> {
        bundle::Entity::find()
            .filter(bundle::Column::DocId.eq(doc_id))
            .order_by_desc(bundle::Column::PublishedAt)
            .one(&self.db)
            .await
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn bundle_exact(
        &self,
        doc_id: &str,
        revision: &str,
    ) -> Result<Option<bundle::Model>, DbErr> {
        bundle::Entity::find()
            .filter(bundle::Column::DocId.eq(doc_id))
            .filter(bundle::Column::Revision.eq(revision))
            .one(&self.db)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn browse(&self, limit: u64, offset: u64) -> Result<Vec<recipe_index::Model>, DbErr> {
        recipe_index::Entity::find()
            .order_by_desc(recipe_index::Column::ModifiedAt)
            .order_by_asc(recipe_index::Column::Id)
            .limit(limit)
            .offset(offset)
            .all(&self.db)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn search(&self, query: &SearchQuery) -> Result<Vec<recipe_index::Model>, DbErr> {
        let mut select = recipe_index::Entity::find();

        if let Some(text) = &query.text {
            select = select.filter(Expr::cust_with_values(
                "search_vector @@ plainto_tsquery('simple', $1)",
                [text.clone()],
            ));
        }
        for cuisine in &query.cuisine {
            select = select.filter(Expr::cust_with_values(
                "cuisine @> $1::jsonb",
                [contains(cuisine)],
            ));
        }
        for course in &query.course {
            select = select.filter(Expr::cust_with_values(
                "course @> $1::jsonb",
                [contains(course)],
            ));
        }
        for diet in &query.diet {
            select = select.filter(Expr::cust_with_values(
                "diet @> $1::jsonb",
                [contains(diet)],
            ));
        }
        for allergen in &query.allergen_free {
            select = select.filter(Expr::cust_with_values(
                "NOT (allergens @> $1::jsonb)",
                [contains(allergen)],
            ));
        }
        if let Some(language) = &query.language {
            select = select.filter(recipe_index::Column::Language.eq(language));
        }

        select
            .order_by_desc(recipe_index::Column::ModifiedAt)
            .order_by_asc(recipe_index::Column::Id)
            .limit(query.limit)
            .offset(query.offset)
            .all(&self.db)
            .await
    }
}

#[cfg(test)]
mod tests;
