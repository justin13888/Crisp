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

use std::collections::BTreeMap;
use std::sync::Arc;

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
use crisp_server::config::{Config, DatabaseOptions, HttpOptions, ModerationOptions};
use crisp_server::platform::PlatformKey;
use crisp_server::quota::Quota;
use crisp_server::state::App;
use crisp_server::storage::Storage;
use kynos::http::StatusCode;
use kynos::router::service::Service;
use kynos::test::{Coverage, TestClient};
use serde_json::{Value, json};

/// The fixed platform seed used by the test server.
pub const PLATFORM_SEED: [u8; 32] = [42u8; 32];

mod mock;
mod moderation;

#[allow(unused_imports)] // Each test binary uses a different subset.
pub use mock::{Fault, Inner, MockStorage};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// The current epoch time, in seconds.
pub fn now() -> i64 {
    crisp_server::util::now_epoch()
}

/// A test configuration.
pub fn test_config() -> Config {
    Config {
        database_url: String::new(),
        database: DatabaseOptions::default(),
        bind_addr: "127.0.0.1:0".into(),
        platform_seed: PLATFORM_SEED,
        session_ttl_secs: 3600,
        challenge_ttl_secs: 300,
        purge_interval_secs: 600,
        http: HttpOptions::default(),
        quota: Quota::default(),
        moderation: ModerationOptions::default(),
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
        unit: Some(UnitCode("g".into())),
        ..Quantity::default()
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
        best_months: None,
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
