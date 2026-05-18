//! End-to-end API tests.
//!
//! These drive the real `crisp-server` router — real handlers, real
//! `crisp-core` cryptography, real A–E verification — over an in-memory
//! [`MockStorage`]. Per the project's testing guidance, the external
//! dependency (PostgreSQL) is mocked so the tests are deterministic and need
//! no database; the SQL layer itself is covered by the gated `postgres_smoke`
//! test when `CRISP_TEST_DATABASE_URL` is set.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use crisp_core::build::{author_sign, finalize_recipe};
use crisp_core::codec::encode_file;
use crisp_core::identity::key_fingerprint;
use crisp_core::model::ids::{DocId, RecipeId, RevisionId};
use crisp_core::model::meta::{Author, DocumentMeta};
use crisp_core::model::payload::{BundlePayload, Kind};
use crisp_core::model::primitives::{LanguageTag, Quantity, Rational, Timestamp, UnitCode};
use crisp_core::model::recipe::{
    AllergenCode, CourseCode, Identity, Ingredient, Recipe, Step, Yield,
};
use crisp_core::model::text::{LocalizedProse, LocalizedText, Paragraph, Prose, Run};
use crisp_core::sign::{SecretKeySet, derive_keyset};
use crisp_core::verify::{Trust, verify};
use crisp_server::config::Config;
use crisp_server::db::{account, bundle, challenge, recipe_index, session};
use crisp_server::platform::PlatformKey;
use crisp_server::state::AppState;
use crisp_server::storage::{SearchQuery, Storage};
use sea_orm::ActiveValue;
use sea_orm::DbErr;
use serde_json::{Value, json};
use tower::ServiceExt;

/// The fixed platform seed used by the test server.
const PLATFORM_SEED: [u8; 32] = [42u8; 32];

// ---------------------------------------------------------------------------
// In-memory mock storage
// ---------------------------------------------------------------------------

/// The mutable state behind [`MockStorage`].
#[derive(Default)]
struct Inner {
    accounts: Vec<account::Model>,
    challenges: Vec<challenge::Model>,
    sessions: Vec<session::Model>,
    bundles: Vec<bundle::Model>,
    recipes: Vec<recipe_index::Model>,
    next_id: i64,
}

/// An in-memory [`Storage`] implementation for tests.
#[derive(Default)]
struct MockStorage {
    inner: Mutex<Inner>,
}

/// Extracts the value of a `Set` active-model field.
fn taken<T>(value: ActiveValue<T>) -> T
where
    T: Into<sea_orm::Value> + sea_orm::sea_query::ValueType + std::fmt::Debug,
{
    value.unwrap()
}

#[async_trait]
impl Storage for MockStorage {
    async fn create_account(&self, account: account::ActiveModel) -> Result<(), DbErr> {
        let model = account::Model {
            usr_id: taken(account.usr_id),
            username: taken(account.username),
            display_name: taken(account.display_name),
            ed25519_pk: taken(account.ed25519_pk),
            ml_dsa_65_pk: taken(account.ml_dsa_65_pk),
            created_at: taken(account.created_at),
        };
        self.inner.lock().unwrap().accounts.push(model);
        Ok(())
    }

    async fn account_by_username(&self, username: &str) -> Result<Option<account::Model>, DbErr> {
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
        let mut inner = self.inner.lock().unwrap();
        if let Some(pos) = inner.challenges.iter().position(|c| c.nonce == nonce) {
            Ok(Some(inner.challenges.remove(pos)))
        } else {
            Ok(None)
        }
    }

    async fn create_session(&self, session: session::ActiveModel) -> Result<(), DbErr> {
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
        self.inner
            .lock()
            .unwrap()
            .sessions
            .retain(|s| s.token != token);
        Ok(())
    }

    async fn bundle_exists(&self, content_hash: &str) -> Result<bool, DbErr> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .bundles
            .iter()
            .any(|b| b.content_hash == content_hash))
    }

    async fn revision_exists(&self, doc_id: &str, revision: &str) -> Result<bool, DbErr> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .bundles
            .iter()
            .any(|b| b.doc_id == doc_id && b.revision == revision))
    }

    async fn insert_bundle(
        &self,
        bundle: bundle::ActiveModel,
        index_rows: Vec<recipe_index::ActiveModel>,
    ) -> Result<(), DbErr> {
        let mut inner = self.inner.lock().unwrap();
        inner.bundles.push(bundle::Model {
            content_hash: taken(bundle.content_hash),
            doc_id: taken(bundle.doc_id),
            revision: taken(bundle.revision),
            kind: taken(bundle.kind),
            format_version: taken(bundle.format_version),
            raw: taken(bundle.raw),
            published_by: taken(bundle.published_by),
            published_at: taken(bundle.published_at),
            endorsed_at: taken(bundle.endorsed_at),
        });
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
        let mut rows = self.inner.lock().unwrap().recipes.clone();
        rows.sort_by_key(|r| std::cmp::Reverse(r.modified_at));
        Ok(rows
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect())
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<recipe_index::Model>, DbErr> {
        let rows = self.inner.lock().unwrap().recipes.clone();
        let matched: Vec<_> = rows
            .into_iter()
            .filter(|r| {
                query
                    .text
                    .as_ref()
                    .is_none_or(|t| r.search_text.to_lowercase().contains(&t.to_lowercase()))
            })
            .filter(|r| query.language.as_ref().is_none_or(|l| &r.language == l))
            .skip(query.offset as usize)
            .take(query.limit as usize)
            .collect();
        Ok(matched)
    }
}

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

/// The current epoch time, in seconds.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Builds a test [`AppState`] over fresh mock storage.
fn test_state() -> AppState {
    AppState {
        storage: Arc::new(MockStorage::default()),
        platform: Arc::new(PlatformKey::from_seed(&PLATFORM_SEED)),
        config: Arc::new(Config {
            database_url: String::new(),
            bind_addr: "127.0.0.1:0".into(),
            platform_seed: PLATFORM_SEED,
            session_ttl_secs: 3600,
            challenge_ttl_secs: 300,
            max_bundle_bytes: 16 * 1024 * 1024,
            cors_permissive: true,
        }),
    }
}

/// Sends a request through a fresh router built from `state`.
async fn send(state: &AppState, request: Request<Body>) -> (StatusCode, Vec<u8>) {
    let response = crisp_server::routes::router(state.clone())
        .oneshot(request)
        .await
        .expect("router response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collect body")
        .to_vec();
    (status, body)
}

/// Sends a JSON `POST` and returns the status and parsed JSON body.
async fn post_json(state: &AppState, uri: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request");
    let (status, bytes) = send(state, request).await;
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// A localized-text map with a single English entry.
fn en_text(value: &str) -> LocalizedText {
    LocalizedText(BTreeMap::from([(
        LanguageTag("en".into()),
        value.to_owned(),
    )]))
}

/// A localized-prose block with a single English paragraph.
fn en_prose(value: &str) -> LocalizedProse {
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

/// Builds a valid, author-signed single-recipe `.crisp` bundle, returning the
/// file bytes and the document id.
fn build_recipe_bundle(author: &SecretKeySet) -> (Vec<u8>, String) {
    let lang = LanguageTag("en".into());
    let fpr = key_fingerprint(&author.public_keyset()).expect("fingerprint");

    let identity = Identity {
        author_fpr: fpr.to_vec(),
        title: "Test Cookies".into(),
        language: lang.clone(),
        created_at: Timestamp(1_000),
        birth_nonce: vec![0u8; 16],
    };
    let amount = |n: i64| Quantity {
        min: Some(Rational::new(n, 1)),
        max: None,
        unit: Some(UnitCode("g".into())),
        approx: None,
        text: None,
    };

    let recipe = Recipe {
        identity: identity.clone(),
        id: RecipeId(String::new()),
        revision: RevisionId(String::new()),
        version: None,
        supersedes: None,
        derived_from: None,
        modified_at: Timestamp(2_000),
        title: en_text("Test Cookies"),
        summary: Some(en_prose("A small batch of test cookies.")),
        r#yield: Yield {
            amount: Quantity {
                min: Some(Rational::new(12, 1)),
                max: None,
                unit: None,
                approx: None,
                text: None,
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
            display: None,
            preparation: None,
            optional: None,
            substitutes: None,
            blob: None,
        }],
        equipment: None,
        steps: vec![Step {
            r#ref: 1,
            group: None,
            text: en_prose("Mix the flour and bake."),
            ingredient_refs: Some(vec![1]),
            equipment_refs: None,
            duration: None,
            temperature: None,
            produces: None,
            blob: None,
        }],
        nutrition: None,
        tags: Some(vec!["dessert".into()]),
        cuisine: Some(vec!["american".into()]),
        course: Some(vec![CourseCode::Dessert]),
        diet: None,
        allergens: Some(vec![AllergenCode::Gluten]),
        media: None,
        notes: None,
        attribution: None,
        ext: None,
    };
    let recipe = finalize_recipe(recipe, fpr).expect("finalize");
    let doc_id = recipe.id.0.clone();

    let payload = BundlePayload {
        v: crisp_core::FORMAT_VERSION,
        min_v: crisp_core::MIN_READER_VERSION,
        kind: Kind::Recipe,
        doc_id: DocId(doc_id.clone()),
        default_language: lang.clone(),
        languages: vec![lang],
        meta: DocumentMeta {
            title: en_text("Test Cookies"),
            authors: vec![Author {
                uid: author.user_id().expect("user id"),
                display_name: "Tester".into(),
                keyset: author.public_keyset(),
            }],
            license: "CC-BY-4.0".into(),
            created_at: Timestamp(1_000),
            modified_at: Timestamp(2_000),
            identity: None,
            description: None,
            cover: None,
        },
        recipes: vec![recipe],
        book: None,
        blobs: BTreeMap::new(),
        ext: None,
    };
    let envelope = author_sign(&payload, author, Timestamp(3_000)).expect("author sign");
    (encode_file(&envelope).expect("encode"), doc_id)
}

/// Registers an account and logs in, returning the bearer token.
async fn register_and_login(state: &AppState, author: &SecretKeySet, username: &str) -> String {
    let keyset = author.public_keyset();
    let (status, body) = post_json(
        state,
        "/api/v1/auth/register",
        json!({
            "username": username,
            "display_name": "Tester",
            "ed25519": hex::encode(&keyset.ed25519),
            "ml_dsa_65": hex::encode(&keyset.ml_dsa_65),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "register: {body}");

    let (status, body) = post_json(
        state,
        "/api/v1/auth/challenge",
        json!({ "username": username }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "challenge: {body}");
    let nonce = body["nonce"].as_str().expect("nonce").to_owned();
    let message = body["message"].as_str().expect("message").to_owned();

    let signature = author.sign(message.as_bytes()).expect("sign challenge");
    let (status, body) = post_json(
        state,
        "/api/v1/auth/login",
        json!({ "nonce": nonce, "signature": hex::encode(&signature) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login: {body}");
    body["token"].as_str().expect("token").to_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_and_server_info_report_versions() {
    let state = test_state();
    let request = Request::builder()
        .uri("/api/v1/server-info")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&state, request).await;
    assert_eq!(status, StatusCode::OK);
    let info: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(info["api_versions"][0], "v1");
    assert_eq!(
        info["format_version"]["current"],
        crisp_core::FORMAT_VERSION
    );
    assert!(
        info["platform_keys"][0]["usr_id"]
            .as_str()
            .unwrap()
            .starts_with("usr1")
    );
}

#[tokio::test]
async fn full_auth_flow_issues_a_working_token() {
    let state = test_state();
    let author = derive_keyset(&[1u8; 32]);
    let token = register_and_login(&state, &author, "alice").await;
    assert_eq!(token.len(), 64);

    // The token authenticates a logout.
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/logout")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&state, request).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn login_rejects_a_signature_from_the_wrong_key() {
    let state = test_state();
    let author = derive_keyset(&[2u8; 32]);
    let imposter = derive_keyset(&[99u8; 32]);
    let keyset = author.public_keyset();

    post_json(
        &state,
        "/api/v1/auth/register",
        json!({
            "username": "bob",
            "display_name": "Bob",
            "ed25519": hex::encode(&keyset.ed25519),
            "ml_dsa_65": hex::encode(&keyset.ml_dsa_65),
        }),
    )
    .await;
    let (_, body) = post_json(
        &state,
        "/api/v1/auth/challenge",
        json!({ "username": "bob" }),
    )
    .await;
    let nonce = body["nonce"].as_str().unwrap().to_owned();
    let message = body["message"].as_str().unwrap().to_owned();

    // The imposter signs the same challenge with a different key.
    let signature = imposter.sign(message.as_bytes()).unwrap();
    let (status, _) = post_json(
        &state,
        "/api/v1/auth/login",
        json!({ "nonce": nonce, "signature": hex::encode(&signature) }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn publish_endorses_indexes_and_serves_a_bundle() {
    let state = test_state();
    let author = derive_keyset(&[3u8; 32]);
    let token = register_and_login(&state, &author, "carol").await;
    let (bundle_bytes, doc_id) = build_recipe_bundle(&author);

    // Publish.
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/bundles")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/crisp+cbor")
        .body(Body::from(bundle_bytes))
        .unwrap();
    let (status, endorsed) = send(&state, request).await;
    assert_eq!(status, StatusCode::CREATED);

    // The returned bytes are endorsed and Published-compliant.
    let platform = PlatformKey::from_seed(&PLATFORM_SEED);
    let report = verify(
        &endorsed,
        std::slice::from_ref(platform.public_keyset()),
        Timestamp(now() as u64),
    );
    assert!(report.compliant, "endorsed bundle should be compliant");
    assert_eq!(report.trust, Trust::Endorsed);

    // Retrieval by doc id returns the stored endorsed bytes.
    let request = Request::builder()
        .uri(format!("/api/v1/bundles/{doc_id}"))
        .body(Body::empty())
        .unwrap();
    let (status, fetched) = send(&state, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched, endorsed);

    // The recipe is now searchable.
    let request = Request::builder()
        .uri("/api/v1/search?q=cookies")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&state, request).await;
    assert_eq!(status, StatusCode::OK);
    let page: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(page["recipes"][0]["doc_id"], doc_id);
}

#[tokio::test]
async fn publish_requires_authentication() {
    let state = test_state();
    let author = derive_keyset(&[4u8; 32]);
    let (bundle_bytes, _) = build_recipe_bundle(&author);
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/bundles")
        .body(Body::from(bundle_bytes))
        .unwrap();
    let (status, _) = send(&state, request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn publish_rejects_a_non_compliant_bundle() {
    let state = test_state();
    let author = derive_keyset(&[5u8; 32]);
    let token = register_and_login(&state, &author, "dave").await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/bundles")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(vec![0x00, 0x01, 0x02]))
        .unwrap();
    let (status, _) = send(&state, request).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

/// Connects to a real PostgreSQL and runs the migration, when
/// `CRISP_TEST_DATABASE_URL` is set. Skipped (passing) otherwise.
#[tokio::test]
async fn postgres_smoke() {
    let Ok(url) = std::env::var("CRISP_TEST_DATABASE_URL") else {
        eprintln!("postgres_smoke skipped: set CRISP_TEST_DATABASE_URL to run it");
        return;
    };
    use sea_orm_migration::MigratorTrait;
    let db = sea_orm::Database::connect(&url).await.expect("connect");
    crisp_server::db::Migrator::up(&db, None)
        .await
        .expect("migrate");
}
