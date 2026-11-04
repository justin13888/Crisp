//! The in-memory [`MockStorage`]: every [`Storage`] operation over plain
//! vectors behind a mutex, with per-operation fault injection.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use crisp_server::db::{account, bundle, challenge, recipe_index, session};
use crisp_server::quota::{Quota, Usage};
use crisp_server::storage::{BundleInsertError, InsertError, PurgeCounts, SearchQuery, Storage};
use sea_orm::ActiveValue;
use sea_orm::DbErr;

use super::now;

/// What `usr_id` stores among `bundles`.
fn usage_of(bundles: &[bundle::Model], usr_id: &str) -> Usage {
    let own = bundles.iter().filter(|b| b.published_by == usr_id);
    Usage {
        bytes: own.clone().map(|b| b.size_bytes as u64).sum(),
        bundles: own.count() as u64,
    }
}

// ---------------------------------------------------------------------------
// In-memory mock storage
// ---------------------------------------------------------------------------

/// A failure a [`MockStorage`] operation can be told to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// The operation fails with a generic database error (→ 500).
    Db,
    /// An insert fails as if a unique constraint rejected it — simulating a
    /// concurrent request that won the race.
    Duplicate,
}

/// The mutable state behind [`MockStorage`].
#[derive(Default)]
pub struct Inner {
    pub accounts: Vec<account::Model>,
    pub challenges: Vec<challenge::Model>,
    pub sessions: Vec<session::Model>,
    pub bundles: Vec<bundle::Model>,
    pub recipes: Vec<recipe_index::Model>,
    next_id: i64,
    faults: HashMap<&'static str, Fault>,
    /// How many times `purge_expired` ran.
    pub purges: u64,
    /// Every query `search` received, in order.
    pub searches: Vec<SearchQuery>,
}

/// An in-memory [`Storage`] implementation for tests.
#[derive(Default)]
pub struct MockStorage {
    pub inner: Mutex<Inner>,
}

/// Extracts the value of a `Set` active-model field.
fn taken<T>(value: ActiveValue<T>) -> T
where
    T: Into<sea_orm::Value> + sea_orm::sea_query::ValueType + std::fmt::Debug,
{
    value.unwrap()
}

impl MockStorage {
    /// Makes `op` fail with `fault` from now on.
    pub fn inject(&self, op: &'static str, fault: Fault) {
        self.inner.lock().unwrap().faults.insert(op, fault);
    }

    /// Clears every injected fault.
    pub fn heal(&self) {
        self.inner.lock().unwrap().faults.clear();
    }

    /// Returns the injected fault for `op`, if any.
    fn fault(&self, op: &'static str) -> Option<Fault> {
        self.inner.lock().unwrap().faults.get(op).copied()
    }

    /// Fails with a database error if `op` has a fault injected.
    fn check(&self, op: &'static str) -> Result<(), DbErr> {
        match self.fault(op) {
            Some(_) => Err(DbErr::Custom(format!("injected fault in {op}"))),
            None => Ok(()),
        }
    }

    /// Fails with the injected insert fault for `op`, if any.
    fn check_insert(&self, op: &'static str) -> Result<(), InsertError> {
        match self.fault(op) {
            Some(Fault::Db) => Err(InsertError::Db(DbErr::Custom(format!(
                "injected fault in {op}"
            )))),
            Some(Fault::Duplicate) => Err(InsertError::Duplicate(format!("injected in {op}"))),
            None => Ok(()),
        }
    }

    /// Inserts a session row directly (e.g. an already-expired one).
    pub fn put_session(&self, token: &str, usr_id: &str, expires_at: i64) {
        self.inner.lock().unwrap().sessions.push(session::Model {
            token: token.to_owned(),
            usr_id: usr_id.to_owned(),
            created_at: 0,
            expires_at,
        });
    }

    /// Whether a session with `token` is stored (expired or not).
    pub fn has_session(&self, token: &str) -> bool {
        self.inner
            .lock()
            .unwrap()
            .sessions
            .iter()
            .any(|s| s.token == token)
    }
}

#[async_trait]
impl Storage for MockStorage {
    async fn create_account(&self, account: account::ActiveModel) -> Result<(), InsertError> {
        self.check_insert("create_account")?;
        let model = account::Model {
            usr_id: taken(account.usr_id),
            username: taken(account.username),
            display_name: taken(account.display_name),
            ed25519_pk: taken(account.ed25519_pk),
            ml_dsa_65_pk: taken(account.ml_dsa_65_pk),
            signed_profile: taken(account.signed_profile),
            created_at: taken(account.created_at),
        };
        let mut inner = self.inner.lock().unwrap();
        if inner
            .accounts
            .iter()
            .any(|a| a.usr_id == model.usr_id || a.username == model.username)
        {
            return Err(InsertError::Duplicate("accounts".into()));
        }
        inner.accounts.push(model);
        Ok(())
    }

    async fn account_by_username(&self, username: &str) -> Result<Option<account::Model>, DbErr> {
        self.check("account_by_username")?;
        Ok(self
            .inner
            .lock()
            .unwrap()
            .accounts
            .iter()
            .find(|a| a.username == username)
            .cloned())
    }

    async fn account_by_usr_id(&self, usr_id: &str) -> Result<Option<account::Model>, DbErr> {
        self.check("account_by_usr_id")?;
        Ok(self
            .inner
            .lock()
            .unwrap()
            .accounts
            .iter()
            .find(|a| a.usr_id == usr_id)
            .cloned())
    }

    async fn create_challenge(&self, challenge: challenge::ActiveModel) -> Result<(), DbErr> {
        self.check("create_challenge")?;
        let model = challenge::Model {
            nonce: taken(challenge.nonce),
            usr_id: taken(challenge.usr_id),
            created_at: taken(challenge.created_at),
            expires_at: taken(challenge.expires_at),
        };
        self.inner.lock().unwrap().challenges.push(model);
        Ok(())
    }

    async fn take_challenge(&self, nonce: &str) -> Result<Option<challenge::Model>, DbErr> {
        self.check("take_challenge")?;
        // Find-and-remove under one lock: atomic, like the SQL `DELETE … RETURNING`.
        let mut inner = self.inner.lock().unwrap();
        Ok(inner
            .challenges
            .iter()
            .position(|c| c.nonce == nonce)
            .map(|pos| inner.challenges.remove(pos)))
    }

    async fn create_session(&self, session: session::ActiveModel) -> Result<(), DbErr> {
        self.check("create_session")?;
        let model = session::Model {
            token: taken(session.token),
            usr_id: taken(session.usr_id),
            created_at: taken(session.created_at),
            expires_at: taken(session.expires_at),
        };
        self.inner.lock().unwrap().sessions.push(model);
        Ok(())
    }

    async fn valid_session(&self, token: &str) -> Result<Option<session::Model>, DbErr> {
        self.check("valid_session")?;
        let now = now();
        Ok(self
            .inner
            .lock()
            .unwrap()
            .sessions
            .iter()
            .find(|s| s.token == token && s.expires_at > now)
            .cloned())
    }

    async fn delete_session(&self, token: &str) -> Result<(), DbErr> {
        self.check("delete_session")?;
        self.inner
            .lock()
            .unwrap()
            .sessions
            .retain(|s| s.token != token);
        Ok(())
    }

    async fn purge_expired(&self, now: i64) -> Result<PurgeCounts, DbErr> {
        self.check("purge_expired")?;
        let mut inner = self.inner.lock().unwrap();
        inner.purges += 1;
        let (sessions, challenges) = (inner.sessions.len(), inner.challenges.len());
        inner.sessions.retain(|s| s.expires_at > now);
        inner.challenges.retain(|c| c.expires_at > now);
        Ok(PurgeCounts {
            sessions: (sessions - inner.sessions.len()) as u64,
            challenges: (challenges - inner.challenges.len()) as u64,
        })
    }

    async fn insert_bundle(
        &self,
        bundle: bundle::ActiveModel,
        index_rows: Vec<recipe_index::ActiveModel>,
        quota: &Quota,
    ) -> Result<(), BundleInsertError> {
        self.check_insert("insert_bundle")?;
        let model = bundle::Model {
            content_hash: taken(bundle.content_hash),
            doc_id: taken(bundle.doc_id),
            revision: taken(bundle.revision),
            kind: taken(bundle.kind),
            format_version: taken(bundle.format_version),
            raw: taken(bundle.raw),
            size_bytes: taken(bundle.size_bytes),
            published_by: taken(bundle.published_by),
            published_at: taken(bundle.published_at),
            endorsed_at: taken(bundle.endorsed_at),
        };
        // One lock covers the check and the insert, as the row lock does in
        // PostgreSQL, so racing inserts are checked one after another.
        let mut inner = self.inner.lock().unwrap();
        let total = inner.bundles.iter().map(|b| b.size_bytes as u64).sum();
        quota.check(
            usage_of(&inner.bundles, &model.published_by),
            Some(total),
            model.size_bytes as u64,
        )?;
        // Mirror the schema's PRIMARY KEY and UNIQUE (doc_id, revision).
        if inner.bundles.iter().any(|b| {
            b.content_hash == model.content_hash
                || (b.doc_id == model.doc_id && b.revision == model.revision)
        }) {
            return Err(InsertError::Duplicate("bundles".into()).into());
        }
        inner.bundles.push(model);
        for row in index_rows {
            inner.next_id += 1;
            let id = inner.next_id;
            inner.recipes.push(recipe_index::Model {
                id,
                bundle_hash: taken(row.bundle_hash),
                doc_id: taken(row.doc_id),
                recipe_id: taken(row.recipe_id),
                recipe_revision: taken(row.recipe_revision),
                title: taken(row.title),
                language: taken(row.language),
                summary: taken(row.summary),
                authors: taken(row.authors),
                tags: taken(row.tags),
                cuisine: taken(row.cuisine),
                course: taken(row.course),
                diet: taken(row.diet),
                allergens: taken(row.allergens),
                license: taken(row.license),
                created_at: taken(row.created_at),
                modified_at: taken(row.modified_at),
                search_text: taken(row.search_text),
            });
        }
        Ok(())
    }

    async fn account_usage(&self, usr_id: &str) -> Result<Usage, DbErr> {
        self.check("account_usage")?;
        Ok(usage_of(&self.inner.lock().unwrap().bundles, usr_id))
    }

    async fn bundle_latest(&self, doc_id: &str) -> Result<Option<bundle::Model>, DbErr> {
        self.check("bundle_latest")?;
        Ok(self
            .inner
            .lock()
            .unwrap()
            .bundles
            .iter()
            .filter(|b| b.doc_id == doc_id)
            .max_by_key(|b| b.published_at)
            .cloned())
    }

    async fn bundle_exact(
        &self,
        doc_id: &str,
        revision: &str,
    ) -> Result<Option<bundle::Model>, DbErr> {
        self.check("bundle_exact")?;
        Ok(self
            .inner
            .lock()
            .unwrap()
            .bundles
            .iter()
            .find(|b| b.doc_id == doc_id && b.revision == revision)
            .cloned())
    }

    async fn browse(&self, limit: u64, offset: u64) -> Result<Vec<recipe_index::Model>, DbErr> {
        self.check("browse")?;
        let mut rows = self.inner.lock().unwrap().recipes.clone();
        rows.sort_by_key(|r| std::cmp::Reverse(r.modified_at));
        Ok(rows
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect())
    }

    /// Records the parsed query and returns every indexed row unfiltered:
    /// matching semantics are the SQL's, asserted in `tests/postgres.rs`.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<recipe_index::Model>, DbErr> {
        self.check("search")?;
        let mut inner = self.inner.lock().unwrap();
        inner.searches.push(query.clone());
        Ok(inner.recipes.clone())
    }
}
