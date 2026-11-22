//! The Crisp server, behind a trait: publishing, downloading, and searching
//! through the spargen-generated client, signed in as the user's identity.
//!
//! Sign-in is challenge → signature → bearer token, done on first need and
//! once more if the server later answers 401. Accounts are never created
//! here: registering is a deliberate act the user takes in the web app.
//!
//! Every request is bounded by [`Limits`]: an overall timeout from connect
//! to the last body byte, and a cap on the response body's size.

mod bounded;

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use crisp_core::sign::login_message;
use schemars::JsonSchema;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{Instrument as _, debug, info, info_span};

use crate::api::{self, types};
use crate::config::ServerConfig;
use crate::signer::Signer;

pub use bounded::{Limits, MAX_RESPONSE_BYTES, REQUEST_TIMEOUT, TooLarge};

/// A server operation that failed, in words an LLM can relay.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct GatewayError(pub String);

/// What to search for. Empty lists do not filter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchQuery {
    /// Free text, e.g. `carbonara`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    /// Cuisine slugs the recipe must have.
    #[serde(default)]
    pub cuisine: Vec<String>,
    /// Course codes the recipe must have.
    #[serde(default)]
    pub course: Vec<String>,
    /// Diet codes the recipe must have.
    #[serde(default)]
    pub diet: Vec<String>,
    /// Allergens the recipe must not declare.
    #[serde(default)]
    pub allergen_free: Vec<String>,
    /// The recipe's language, e.g. `en`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// At most this many results (1–200, default 20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

/// One published recipe a search found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchHit {
    /// The document id, usable as `based_on`.
    pub doc_id: String,
    /// The title.
    pub title: String,
    /// The title's language.
    pub language: String,
    /// The authors' display names.
    pub authors: Vec<String>,
    /// Cuisine slugs.
    pub cuisine: Vec<String>,
    /// Course codes.
    pub course: Vec<String>,
    /// Diet codes.
    pub diet: Vec<String>,
    /// Free tags.
    pub tags: Vec<String>,
}

/// A Crisp server.
#[async_trait]
pub trait Gateway: Send + Sync {
    /// The server's URL, or `None` when no server is configured.
    fn url(&self) -> Option<String>;

    /// Publishes an author-signed file; returns the endorsed file.
    async fn publish(&self, bundle: Vec<u8>) -> Result<Vec<u8>, GatewayError>;

    /// The latest published revision of `doc_id`, or `None`.
    async fn download(&self, doc_id: &str) -> Result<Option<Vec<u8>>, GatewayError>;

    /// Searches published recipes.
    async fn search(&self, query: SearchQuery) -> Result<Vec<SearchHit>, GatewayError>;
}

/// The gateway when no server is configured: every call explains the setup.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoServer;

const NO_SERVER: &str = "no Crisp server is configured: set CRISP_SERVER_URL and \
                         CRISP_SERVER_USERNAME (an account registered in the Crisp app)";

#[async_trait]
impl Gateway for NoServer {
    fn url(&self) -> Option<String> {
        None
    }

    async fn publish(&self, _bundle: Vec<u8>) -> Result<Vec<u8>, GatewayError> {
        Err(GatewayError(NO_SERVER.into()))
    }

    async fn download(&self, _doc_id: &str) -> Result<Option<Vec<u8>>, GatewayError> {
        Err(GatewayError(NO_SERVER.into()))
    }

    async fn search(&self, _query: SearchQuery) -> Result<Vec<SearchHit>, GatewayError> {
        Err(GatewayError(NO_SERVER.into()))
    }
}

/// A real server over HTTP.
pub struct HttpGateway {
    config: ServerConfig,
    limits: Limits,
    client: api::Client,
    signer: Arc<Signer>,
    session: Mutex<Option<api::Client>>,
}

impl std::fmt::Debug for HttpGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpGateway")
            .field("url", &self.config.url)
            .finish_non_exhaustive()
    }
}

/// The query string for a search: blank text and empty filters are left
/// out, lists are comma-joined, and the page size defaults to 20.
fn search_params(query: SearchQuery) -> api::SearchParams {
    let joined = |values: &[String]| (!values.is_empty()).then(|| values.join(","));
    api::SearchParams {
        q: query.q.filter(|q| !q.trim().is_empty()),
        cuisine: joined(&query.cuisine),
        course: joined(&query.course),
        diet: joined(&query.diet),
        allergen_free: joined(&query.allergen_free),
        language: query.language,
        limit: Some(query.limit.unwrap_or(20)),
        offset: None,
    }
}

/// What a failed download's status means: 404 is "no such recipe"; 410
/// is a recipe the server took down; anything else is a failure.
fn download_status(
    doc_id: &str,
    status: Option<u16>,
) -> Option<Result<Option<Vec<u8>>, GatewayError>> {
    match status {
        Some(404) => Some(Ok(None)),
        Some(410) => Some(Err(GatewayError(format!(
            "{doc_id} was taken down or unpublished on the server (410)"
        )))),
        _ => None,
    }
}

/// Describes a call that failed with the server's answer, or without one:
/// the status when there is one, then the error.
fn answered<E: std::fmt::Display>(what: &str, error: &api::Error<E>) -> GatewayError {
    match error.status().map(|s| s.as_u16()) {
        Some(401) => GatewayError(format!("{what}: the server refused the sign-in (401)")),
        Some(status) => GatewayError(format!("{what}: the server answered {status}: {error}")),
        None => GatewayError(format!("{what}: {error}")),
    }
}

impl HttpGateway {
    /// A gateway for `config`, signing in with `signer`, under the default
    /// [`Limits`].
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] for a malformed server URL.
    pub fn new(config: ServerConfig, signer: Arc<Signer>) -> Result<Self, GatewayError> {
        Self::with_limits(config, signer, Limits::default())
    }

    /// A gateway for `config`, signing in with `signer`, whose every request
    /// is bounded by `limits`.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] for a malformed server URL, or when the
    /// HTTP client cannot start (its TLS backend fails to initialise).
    pub fn with_limits(
        config: ServerConfig,
        signer: Arc<Signer>,
        limits: Limits,
    ) -> Result<Self, GatewayError> {
        let client = limits.client(&config.url)?;
        Ok(Self {
            config,
            limits,
            client,
            signer,
            session: Mutex::new(None),
        })
    }

    /// Describes a call to `what` that started at `started` and failed: a
    /// timeout or an oversized response by its limit, else the status when
    /// there is one, then the error.
    fn failure<E: std::fmt::Display>(
        &self,
        what: &str,
        started: Instant,
        error: &api::Error<E>,
    ) -> GatewayError {
        self.limits
            .exceeded(what, started, error)
            .unwrap_or_else(|| answered(what, error))
    }

    /// A sign-in step's [`Self::failure`]: when the server answered, it adds
    /// `hint` about registering the identity.
    fn sign_in_failure<E: std::fmt::Display>(
        &self,
        what: &str,
        started: Instant,
        error: &api::Error<E>,
        hint: &str,
    ) -> GatewayError {
        self.limits
            .exceeded(what, started, error)
            .unwrap_or_else(|| {
                let mut failed = answered(what, error);
                failed.0.push_str(&format!(
                " (is {:?} registered on this server{hint}? Register it in the Crisp app first)",
                self.config.username
            ));
                failed
            })
    }

    /// A signed-in client, signing in when there is no session yet.
    async fn signed_in(&self) -> Result<api::Client, GatewayError> {
        let mut session = self.session.lock().await;
        if let Some(client) = session.as_ref() {
            return Ok(client.clone());
        }
        let span = info_span!("sign_in", username = %self.config.username);
        let client = self.sign_in().instrument(span).await?;
        *session = Some(client.clone());
        Ok(client)
    }

    async fn sign_in(&self) -> Result<api::Client, GatewayError> {
        let secret = self.signer.unlock().await.map_err(|e| GatewayError(e.0))?;
        let started = Instant::now();
        let challenge = self
            .client
            .challenge(&types::ChallengeRequest {
                username: self.config.username.clone(),
            })
            .await
            .map_err(|e| self.sign_in_failure("starting sign-in", started, &e, ""))?
            .into_inner();
        let nonce = hex::decode(&challenge.nonce)
            .map_err(|e| GatewayError(format!("the server sent a malformed challenge: {e}")))?;
        let message = login_message(&challenge.audience, &nonce)
            .map_err(|e| GatewayError(format!("could not build the sign-in message: {e}")))?;
        let signature = secret
            .sign(&message)
            .map_err(|e| GatewayError(format!("could not sign in: {e}")))?;
        let session = self
            .client
            .login(&types::LoginRequest {
                nonce: challenge.nonce,
                signature: hex::encode(signature),
            })
            .await
            .map_err(|e| {
                self.sign_in_failure("signing in", started, &e, " with the key in CRISP_KEY_FILE")
            })?
            .into_inner();
        info!(expires_at = session.expires_at, "signed in");
        Ok(self.client.clone().with_credential(
            "sessionToken",
            api::Credential::Bearer(SecretString::from(session.token)),
        ))
    }

    async fn forget_session(&self) {
        debug!("the session was refused; signing in again");
        *self.session.lock().await = None;
    }
}

#[async_trait]
impl Gateway for HttpGateway {
    fn url(&self) -> Option<String> {
        Some(self.config.url.clone())
    }

    async fn publish(&self, bundle: Vec<u8>) -> Result<Vec<u8>, GatewayError> {
        let body = bytes::Bytes::from(bundle);
        // A refused session (expired, or signed out elsewhere) earns exactly
        // one fresh sign-in; any other answer is final.
        let client = self.signed_in().await?;
        let mut started = Instant::now();
        let answer = match client.publish(&body).await {
            Err(e) if e.status().map(|s| s.as_u16()) == Some(401) => {
                self.forget_session().await;
                let client = self.signed_in().await?;
                started = Instant::now();
                client.publish(&body).await
            }
            first => first,
        };
        let endorsed = answer
            .map_err(|e| self.failure("publishing", started, &e))?
            .into_inner()
            .to_vec();
        info!(bytes = endorsed.len(), "published");
        Ok(endorsed)
    }

    async fn download(&self, doc_id: &str) -> Result<Option<Vec<u8>>, GatewayError> {
        let started = Instant::now();
        match self.client.get_latest(doc_id.to_owned()).await {
            Ok(bytes) => Ok(Some(bytes.into_inner().to_vec())),
            Err(e) => download_status(doc_id, e.status().map(|s| s.as_u16()))
                .unwrap_or_else(|| Err(self.failure("downloading", started, &e))),
        }
    }

    async fn search(&self, query: SearchQuery) -> Result<Vec<SearchHit>, GatewayError> {
        let params = search_params(query);
        let started = Instant::now();
        let page = self
            .client
            .search(params)
            .await
            .map_err(|e| self.failure("searching", started, &e))?
            .into_inner();
        Ok(page
            .recipes
            .into_iter()
            .map(|r| SearchHit {
                doc_id: r.doc_id,
                title: r.title,
                language: r.language,
                authors: r.authors.into_iter().map(|a| a.name).collect(),
                cuisine: r.cuisine,
                course: r.course,
                diet: r.diet,
                tags: r.tags,
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "gateway_tests.rs"]
mod tests;
