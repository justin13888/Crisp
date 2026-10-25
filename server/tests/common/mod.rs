//! Shared test support: an in-memory [`MockStorage`] with fault injection,
//! bundle/profile fixtures, and helpers that drive the real router through
//! kynos's in-process [`TestClient`].
//!
//! Per the project's testing guidance, the external dependency (PostgreSQL)
//! is mocked so the API tests are deterministic and need no database. The mock
//! does not re-implement search matching - it records the parsed query - so
//! search semantics and the rest of the SQL layer are covered by
//! `tests/postgres.rs` when `CRISP_TEST_DATABASE_URL` is set.

#![allow(dead_code)] // Each test binary uses a different subset.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use crisp_core::author::seal;
use crisp_core::codec::encode_file;
use crisp_core::model::ids::{DocId, RecipeId, RevisionId};
use crisp_core::model::meta::DocumentMeta;
use crisp_core::model::payload::{BundlePayload, Kind};
use crisp_core::model::primitives::{
    LanguageTag, OpenCode, Quantity, Rational, Timestamp, UnitCode,
};
use crisp_core::model::recipe::{
    AllergenCode, CourseCode, Identity, Ingredient, Recipe, Step, StepIngredient, Yield,
};
use crisp_core::model::text::{LocalizedProse, LocalizedText, Paragraph, Prose, Run};
use crisp_core::sign::SecretKeySet;
use crisp_server::config::{Config, HttpOptions};
use crisp_server::db::{account, bundle, challenge, recipe_index, session};
use crisp_server::platform::PlatformKey;
use crisp_server::state::App;
use crisp_server::storage::{InsertError, PurgeCounts, SearchQuery, Storage};
use kynos::http::StatusCode;
use kynos::router::service::Service;
use kynos::test::{Coverage, TestClient};
use sea_orm::ActiveValue;
use sea_orm::DbErr;
use serde_json::{Value, json};

/// The fixed platform seed used by the test server.
pub const PLATFORM_SEED: [u8; 32] = [42u8; 32];

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
    ) -> Result<(), InsertError> {
        self.check_insert("insert_bundle")?;
        let model = bundle::Model {
            content_hash: taken(bundle.content_hash),
            doc_id: taken(bundle.doc_id),
            revision: taken(bundle.revision),
            kind: taken(bundle.kind),
            format_version: taken(bundle.format_version),
            raw: taken(bundle.raw),
            published_by: taken(bundle.published_by),
            published_at: taken(bundle.published_at),
            endorsed_at: taken(bundle.endorsed_at),
        };
        let mut inner = self.inner.lock().unwrap();
        // Mirror the schema's PRIMARY KEY and UNIQUE (doc_id, revision).
        if inner.bundles.iter().any(|b| {
            b.content_hash == model.content_hash
                || (b.doc_id == model.doc_id && b.revision == model.revision)
        }) {
            return Err(InsertError::Duplicate("bundles".into()));
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

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// The current epoch time, in seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A test configuration.
pub fn test_config() -> Config {
    Config {
        database_url: String::new(),
        bind_addr: "127.0.0.1:0".into(),
        platform_seed: PLATFORM_SEED,
        session_ttl_secs: 3600,
        challenge_ttl_secs: 300,
        purge_interval_secs: 600,
        http: HttpOptions::default(),
    }
}

/// The platform key the test server endorses with.
pub fn platform() -> PlatformKey {
    PlatformKey::from_seed(&PLATFORM_SEED).expect("platform key")
}

/// A test server: the in-process client plus a handle on its mock storage.
pub struct Harness {
    pub client: TestClient<App>,
    pub storage: Arc<MockStorage>,
}

impl Harness {
    /// A server over fresh mock storage with the default test configuration.
    pub fn new() -> Self {
        Self::with_config(test_config())
    }

    /// A server over fresh mock storage with `config`.
    pub fn with_config(config: Config) -> Self {
        Self::build(config, Arc::new(MockStorage::default()))
    }

    /// A server over `storage` with `config`.
    pub fn build(config: Config, storage: Arc<MockStorage>) -> Self {
        let client = TestClient::new(Self::service(config, &storage));
        Self { client, storage }
    }

    /// A server whose every exchange is also recorded into `coverage`.
    pub fn covered(config: Config, storage: Arc<MockStorage>, coverage: &Coverage) -> Self {
        let client = TestClient::new(Self::service(config, &storage)).with_coverage(coverage);
        Self { client, storage }
    }

    /// Builds the real router over `storage`.
    fn service(config: Config, storage: &Arc<MockStorage>) -> Service<App> {
        crisp_server::router(&config.http)
            .build(App::new(
                Arc::clone(storage) as Arc<dyn Storage>,
                Arc::new(platform()),
                Arc::new(config),
            ))
            .expect("router builds")
    }

    /// Registers `username` for `author`, asserting success; returns the `usr…` id.
    pub async fn register(&self, author: &SecretKeySet, username: &str) -> String {
        let response = self
            .client
            .post("/api/v1/auth/register")
            .json(&json!({ "profile": signed_profile(author, username, "Tester") }))
            .send()
            .await;
        response.assert_status(StatusCode::OK);
        let body: Value = response.json();
        body["usr_id"].as_str().expect("usr_id").to_owned()
    }

    /// Requests a challenge for `username`, returning `(nonce, audience)`.
    pub async fn challenge(&self, username: &str) -> (String, String) {
        let response = self
            .client
            .post("/api/v1/auth/challenge")
            .json(&json!({ "username": username }))
            .send()
            .await;
        response.assert_status(StatusCode::OK);
        let body: Value = response.json();
        (
            body["nonce"].as_str().expect("nonce").to_owned(),
            body["audience"].as_str().expect("audience").to_owned(),
        )
    }

    /// Answers a challenge with `signer`'s signature, bound to `audience`.
    pub async fn login(
        &self,
        signer: &SecretKeySet,
        nonce: &str,
        audience: &str,
    ) -> kynos::test::TestResponse {
        let nonce_bytes = hex::decode(nonce).expect("hex nonce");
        let message = crisp_core::sign::login_message(audience, &nonce_bytes).expect("message");
        let signature = signer.sign(&message).expect("sign challenge");
        self.client
            .post("/api/v1/auth/login")
            .json(&json!({ "nonce": nonce, "signature": hex::encode(&signature) }))
            .send()
            .await
    }

    /// Registers an account and logs in, returning the bearer token.
    pub async fn register_and_login(&self, author: &SecretKeySet, username: &str) -> String {
        self.register(author, username).await;
        let (nonce, message) = self.challenge(username).await;
        let response = self.login(author, &nonce, &message).await;
        response.assert_status(StatusCode::OK);
        let body: Value = response.json();
        body["token"].as_str().expect("token").to_owned()
    }

    /// Uploads `bytes` as a bundle with `token`.
    pub async fn publish(&self, token: &str, bytes: Vec<u8>) -> kynos::test::TestResponse {
        self.client
            .post("/api/v1/bundles")
            .header("authorization", &format!("Bearer {token}"))
            .body("application/octet-stream", bytes)
            .send()
            .await
    }
}

/// Asserts a problem+json response carries the expected stable `code`.
pub fn assert_code(response: &kynos::test::TestResponse, code: &str) {
    assert_eq!(
        response.header("content-type"),
        Some("application/problem+json"),
        "expected a problem document"
    );
    let body: Value = response.json();
    assert_eq!(body["code"], code, "unexpected problem: {body}");
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A localized-text map with a single English entry.
pub fn en_text(value: &str) -> LocalizedText {
    LocalizedText(BTreeMap::from([(
        LanguageTag("en".into()),
        value.to_owned(),
    )]))
}

/// A localized-prose block with a single English paragraph.
pub fn en_prose(value: &str) -> LocalizedProse {
    let run = Run {
        t: value.to_owned(),
        style: None,
        link: None,
        reference: None,
    };
    LocalizedProse(BTreeMap::from([(
        LanguageTag("en".into()),
        Prose(vec![Paragraph(vec![run])]),
    )]))
}

/// Options for [`build_recipe_bundle_with`].
#[derive(Debug, Clone)]
pub struct RecipeSpec {
    /// The recipe title.
    pub title: String,
    /// Cuisine tags.
    pub cuisine: Vec<String>,
    /// The recipe's `modified_at`; distinct values yield distinct revisions.
    pub modified_at: u64,
    /// Declared allergens; empty declares none.
    pub allergens: Vec<AllergenCode>,
}

impl Default for RecipeSpec {
    fn default() -> Self {
        Self {
            title: "Test Cookies".into(),
            cuisine: vec!["american".into()],
            modified_at: 2_000,
            allergens: vec![AllergenCode::Gluten],
        }
    }
}

/// Builds a valid, author-signed single-recipe `.crisp` bundle, returning the
/// file bytes and the document id.
pub fn build_recipe_bundle(author: &SecretKeySet) -> (Vec<u8>, String) {
    let (bytes, doc_id, _) = build_recipe_bundle_with(author, &RecipeSpec::default());
    (bytes, doc_id)
}

/// Builds a valid, author-signed single-recipe bundle from `spec`, returning
/// the file bytes, the document id, and the recipe revision id.
pub fn build_recipe_bundle_with(
    author: &SecretKeySet,
    spec: &RecipeSpec,
) -> (Vec<u8>, String, String) {
    let lang = LanguageTag("en".into());
    let amount = |n: i64| Quantity {
        min: Some(Rational::new(n, 1)),
        max: None,
        unit: Some(UnitCode("g".into())),
        approx: None,
        text: None,
    };
    let recipe = Recipe {
        identity: Identity {
            author_fpr: Vec::new(),
            title: spec.title.clone(),
            language: lang.clone(),
            created_at: Timestamp(1_000),
            birth_nonce: vec![0u8; 16],
        },
        id: RecipeId(String::new()),
        revision: RevisionId(String::new()),
        supersedes: None,
        derived_from: None,
        modified_at: Timestamp(0),
        title: en_text(&spec.title),
        summary: Some(en_prose("A small batch of test cookies.")),
        r#yield: Yield {
            amount: Quantity {
                min: Some(Rational::new(12, 1)),
                ..Quantity::default()
            },
            unit_text: en_text("cookies"),
        },
        times: None,
        ingredient_groups: None,
        ingredients: vec![Ingredient {
            r#ref: 1,
            group: None,
            name: en_text("Flour"),
            quantity: Some(amount(200)),
            alt: None,
            fixed: None,
            preparation: None,
            note: None,
            optional: None,
            substitutes: None,
            recipe: None,
            blob: None,
            ext: None,
        }],
        equipment: None,
        step_groups: None,
        steps: vec![Step {
            r#ref: 1,
            group: None,
            text: en_prose("Mix the flour and bake."),
            ingredients: Some(vec![StepIngredient {
                r#ref: 1,
                quantity: None,
            }]),
            equipment: None,
            timers: None,
            heat: None,
            temperature: None,
            produces: None,
            note: None,
            blob: None,
            ext: None,
        }],
        nutrition: None,
        tags: Some(vec!["dessert".into()]),
        cuisine: Some(spec.cuisine.clone()),
        course: Some(vec![OpenCode::Known(CourseCode::Dessert)]),
        diet: None,
        allergens: (!spec.allergens.is_empty()).then(|| {
            spec.allergens
                .iter()
                .copied()
                .map(OpenCode::Known)
                .collect()
        }),
        media: None,
        notes: None,
        source: None,
        attribution: None,
        ext: None,
    };
    let draft = BundlePayload {
        v: crisp_core::FORMAT_VERSION,
        min_v: crisp_core::MIN_READER_VERSION,
        kind: Kind::Recipe,
        doc_id: DocId(String::new()),
        default_language: lang.clone(),
        languages: vec![lang],
        meta: DocumentMeta {
            title: en_text(&spec.title),
            authors: Vec::new(),
            license: "CC-BY-4.0".into(),
            created_at: Timestamp(1_000),
            modified_at: Timestamp(0),
            identity: None,
            description: None,
            cover: None,
            ext: None,
        },
        recipes: vec![recipe],
        book: None,
        blobs: BTreeMap::new(),
        ext: None,
    };
    let bytes = seal(&draft, author, "Tester", Timestamp(spec.modified_at), None).expect("seal");
    let sealed = crisp_core::author::open_draft(&bytes).expect("open");
    let doc_id = sealed.doc_id.0.clone();
    let revision = sealed.recipes[0].revision.0.clone();
    (bytes, doc_id, revision)
}

/// Endorses an author-signed bundle with the platform key derived from
/// `seed`, as a server would.
///
/// Runs on a dedicated thread: unoptimized ML-DSA signing needs more stack
/// than is left on a test thread already holding a large async test body.
pub fn endorse_with(seed: [u8; 32], bytes: &[u8]) -> Vec<u8> {
    let bytes = bytes.to_vec();
    std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(move || {
            let platform = PlatformKey::from_seed(&seed).expect("platform key");
            let envelope = crisp_core::codec::decode_file(&bytes).expect("decode");
            let endorsed = platform.endorse(&envelope, now()).expect("endorse");
            encode_file(&endorsed).expect("encode")
        })
        .expect("spawn signing thread")
        .join()
        .expect("signing thread")
}

/// Builds a hex-encoded `signed-profile` for use as a register request body.
pub fn signed_profile(author: &SecretKeySet, username: &str, display_name: &str) -> String {
    let bytes = crisp_core::profile::sign_profile(author, username, display_name, Timestamp(1_000))
        .expect("sign profile");
    hex::encode(bytes)
}
