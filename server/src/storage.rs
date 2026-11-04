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
    QuerySelect, RuntimeErr, SqlErr, SqlxError, TransactionTrait,
};
use serde_json::json;

use crate::db::{account, bundle, challenge, recipe_index, session};
use crate::quota::{Breach, Quota, Usage};

mod quota;
mod sessions;

pub use sessions::session_digest;

/// A failure inserting a row whose uniqueness the schema enforces.
#[derive(Debug, thiserror::Error)]
pub enum InsertError {
    /// A unique constraint (or primary key) rejected the row: the record
    /// already exists, possibly inserted by a concurrent request.
    #[error("unique constraint violated: {0}")]
    Duplicate(String),
    /// Any other database failure.
    #[error(transparent)]
    Db(#[from] DbErr),
}

/// A failure storing a bundle: an insert failure, or a storage quota the
/// bundle does not fit.
#[derive(Debug, thiserror::Error)]
pub enum BundleInsertError {
    /// Storing the bundle would breach a storage quota; nothing was written.
    #[error(transparent)]
    Quota(#[from] Breach),
    /// The insert itself failed.
    #[error(transparent)]
    Insert(#[from] InsertError),
}

impl From<DbErr> for BundleInsertError {
    fn from(error: DbErr) -> Self {
        Self::Insert(InsertError::Db(error))
    }
}

impl InsertError {
    /// Classifies a database error, recognizing unique-constraint violations.
    #[must_use]
    pub fn classify(error: DbErr) -> Self {
        match error.sql_err() {
            Some(SqlErr::UniqueConstraintViolation(detail)) => Self::Duplicate(detail),
            _ => Self::Db(error),
        }
    }
}

/// Whether `error` means the database could not be reached or could not
/// serve the statement in time, rather than that the statement was wrong.
///
/// These are the failures a client should retry later (an outage, an
/// exhausted pool, a restart, a statement cut off by `statement_timeout`),
/// so the API answers them 503 instead of 500. Recognized: a pool acquire
/// failure, a connection error, an I/O, TLS, or protocol failure underneath
/// a statement, and the PostgreSQL SQLSTATE classes 08 (connection
/// exception), 53 (insufficient resources), and 57 (operator intervention,
/// which includes `57014 query_canceled` and the shutdown codes).
#[must_use]
pub fn is_unavailable(error: &DbErr) -> bool {
    match error {
        DbErr::ConnectionAcquire(_) | DbErr::Conn(_) => true,
        DbErr::Exec(RuntimeErr::SqlxError(error)) | DbErr::Query(RuntimeErr::SqlxError(error)) => {
            match error {
                SqlxError::Io(_)
                | SqlxError::Tls(_)
                | SqlxError::Protocol(_)
                | SqlxError::PoolTimedOut
                | SqlxError::PoolClosed
                | SqlxError::WorkerCrashed => true,
                SqlxError::Database(error) => error
                    .code()
                    .is_some_and(|code| ["08", "53", "57"].iter().any(|c| code.starts_with(c))),
                _ => false,
            }
        }
        _ => false,
    }
}

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

/// The repository abstraction over persistent storage.
// `async_trait` marks each boxed-future method `#[must_use]`, which clippy then
// flags as redundant on a `Result`-returning method; the attribute is not ours.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait Storage: Send + Sync + 'static {
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
    /// not expired.
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
