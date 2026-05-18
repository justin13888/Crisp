//! The repository layer: a mockable [`Storage`] trait and its PostgreSQL
//! implementation [`PgStorage`].
//!
//! Every database access goes through [`Storage`], so handlers depend on the
//! trait, not on SeaORM. Tests can substitute an in-memory implementation;
//! production uses [`PgStorage`]. Bundle publication writes the bundle and
//! its index rows inside one transaction, so a partially-indexed bundle can
//! never be observed.

use async_trait::async_trait;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
};
use serde_json::json;

use crate::db::{account, bundle, challenge, recipe_index, session};
use crate::util::now_epoch;

/// A full-text and faceted recipe search request.
#[derive(Debug, Clone, Default)]
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
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// Inserts a new account.
    async fn create_account(&self, account: account::ActiveModel) -> Result<(), DbErr>;
    /// Looks an account up by username.
    async fn account_by_username(&self, username: &str) -> Result<Option<account::Model>, DbErr>;
    /// Looks an account up by its `usr…` id.
    async fn account_by_usr_id(&self, usr_id: &str) -> Result<Option<account::Model>, DbErr>;

    /// Stores a pending login challenge.
    async fn create_challenge(&self, challenge: challenge::ActiveModel) -> Result<(), DbErr>;
    /// Atomically fetches and consumes a login challenge by its nonce.
    async fn take_challenge(&self, nonce: &str) -> Result<Option<challenge::Model>, DbErr>;

    /// Stores a freshly issued session.
    async fn create_session(&self, session: session::ActiveModel) -> Result<(), DbErr>;
    /// Fetches a session by token, only if it has not expired.
    async fn valid_session(&self, token: &str) -> Result<Option<session::Model>, DbErr>;
    /// Deletes a session, ending it.
    async fn delete_session(&self, token: &str) -> Result<(), DbErr>;

    /// Whether a bundle with this content hash is already stored.
    async fn bundle_exists(&self, content_hash: &str) -> Result<bool, DbErr>;
    /// Whether this `(doc_id, revision)` pair is already published.
    async fn revision_exists(&self, doc_id: &str, revision: &str) -> Result<bool, DbErr>;
    /// Inserts a bundle and its index rows in a single transaction.
    async fn insert_bundle(
        &self,
        bundle: bundle::ActiveModel,
        index_rows: Vec<recipe_index::ActiveModel>,
    ) -> Result<(), DbErr>;
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
    async fn create_account(&self, account: account::ActiveModel) -> Result<(), DbErr> {
        account.insert(&self.db).await?;
        Ok(())
    }

    async fn account_by_username(&self, username: &str) -> Result<Option<account::Model>, DbErr> {
        account::Entity::find()
            .filter(account::Column::Username.eq(username))
            .one(&self.db)
            .await
    }

    async fn account_by_usr_id(&self, usr_id: &str) -> Result<Option<account::Model>, DbErr> {
        account::Entity::find_by_id(usr_id.to_owned())
            .one(&self.db)
            .await
    }

    async fn create_challenge(&self, challenge: challenge::ActiveModel) -> Result<(), DbErr> {
        challenge.insert(&self.db).await?;
        Ok(())
    }

    async fn take_challenge(&self, nonce: &str) -> Result<Option<challenge::Model>, DbErr> {
        let found = challenge::Entity::find_by_id(nonce.to_owned())
            .one(&self.db)
            .await?;
        if found.is_some() {
            challenge::Entity::delete_by_id(nonce.to_owned())
                .exec(&self.db)
                .await?;
        }
        Ok(found)
    }

    async fn create_session(&self, session: session::ActiveModel) -> Result<(), DbErr> {
        session.insert(&self.db).await?;
        Ok(())
    }

    async fn valid_session(&self, token: &str) -> Result<Option<session::Model>, DbErr> {
        session::Entity::find_by_id(token.to_owned())
            .filter(session::Column::ExpiresAt.gt(now_epoch()))
            .one(&self.db)
            .await
    }

    async fn delete_session(&self, token: &str) -> Result<(), DbErr> {
        session::Entity::delete_by_id(token.to_owned())
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn bundle_exists(&self, content_hash: &str) -> Result<bool, DbErr> {
        let count = bundle::Entity::find_by_id(content_hash.to_owned())
            .count(&self.db)
            .await?;
        Ok(count > 0)
    }

    async fn revision_exists(&self, doc_id: &str, revision: &str) -> Result<bool, DbErr> {
        let count = bundle::Entity::find()
            .filter(bundle::Column::DocId.eq(doc_id))
            .filter(bundle::Column::Revision.eq(revision))
            .count(&self.db)
            .await?;
        Ok(count > 0)
    }

    async fn insert_bundle(
        &self,
        bundle: bundle::ActiveModel,
        index_rows: Vec<recipe_index::ActiveModel>,
    ) -> Result<(), DbErr> {
        let txn = self.db.begin().await?;
        bundle.insert(&txn).await?;
        for row in index_rows {
            row.insert(&txn).await?;
        }
        txn.commit().await?;
        Ok(())
    }

    async fn bundle_latest(&self, doc_id: &str) -> Result<Option<bundle::Model>, DbErr> {
        bundle::Entity::find()
            .filter(bundle::Column::DocId.eq(doc_id))
            .order_by_desc(bundle::Column::PublishedAt)
            .one(&self.db)
            .await
    }

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

    async fn browse(&self, limit: u64, offset: u64) -> Result<Vec<recipe_index::Model>, DbErr> {
        recipe_index::Entity::find()
            .order_by_desc(recipe_index::Column::ModifiedAt)
            .limit(limit)
            .offset(offset)
            .all(&self.db)
            .await
    }

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
            .limit(query.limit)
            .offset(query.offset)
            .all(&self.db)
            .await
    }
}
