//! Crypto-bound accounts: registration, login challenge, and login
//! (plan Stage 12).
//!
//! An account *is* a Composite ML-DSA key set. The client derives that key
//! deterministically from `(username, passphrase)` via Argon2id (in
//! `crisp-core`); the passphrase never leaves the client. Registration stores
//! the username and the **public** key set — whose hash is the `usr…` id.
//! Login is a signed-challenge handshake: the server hands out a nonce, the
//! client signs it with the secret key, and the server verifies the signature
//! against the stored public key set. Possessing the account therefore means
//! possessing the key, which means knowing the credentials.

use axum::Json;
use axum::extract::{FromRequestParts, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use crisp_core::identity::derive_user_id;
use crisp_core::model::meta::PublicKeySet;
use crisp_core::sign::verify_composite;
use serde::{Deserialize, Serialize};

use crate::db::{account, challenge, session};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::util::{now_epoch, random_token};
use sea_orm::ActiveValue::Set;

/// Decodes a hex string into bytes, mapping failure to a 400.
fn from_hex(label: &str, value: &str) -> ApiResult<Vec<u8>> {
    hex::decode(value.trim()).map_err(|_| ApiError::BadRequest(format!("{label} is not valid hex")))
}

/// Builds the exact byte string a client signs to answer a login challenge.
/// The `crisp/login/1` prefix domain-separates it from a bundle signature.
#[must_use]
pub fn login_message(nonce: &str) -> String {
    format!("crisp/login/1:{nonce}")
}

/// The `POST /api/v1/auth/register` request body.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    /// The chosen username — also the client-side Argon2id salt.
    pub username: String,
    /// A human-readable display name.
    pub display_name: String,
    /// The Ed25519 public key, hex-encoded (32 bytes).
    pub ed25519: String,
    /// The ML-DSA-65 public key, hex-encoded (1952 bytes).
    pub ml_dsa_65: String,
}

/// The `POST /api/v1/auth/register` response body.
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    /// The derived `usr…` account id.
    pub usr_id: String,
}

/// The `POST /api/v1/auth/challenge` request body.
#[derive(Debug, Deserialize)]
pub struct ChallengeRequest {
    /// The username to authenticate.
    pub username: String,
}

/// The `POST /api/v1/auth/challenge` response body.
#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    /// The `usr…` id being challenged.
    pub usr_id: String,
    /// The single-use challenge nonce.
    pub nonce: String,
    /// The exact string the client must sign with its secret key.
    pub message: String,
}

/// The `POST /api/v1/auth/login` request body.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// The nonce from the matching challenge.
    pub nonce: String,
    /// The composite signature over [`login_message`], hex-encoded.
    pub signature: String,
}

/// The `POST /api/v1/auth/login` response body.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// The authenticated `usr…` id.
    pub usr_id: String,
    /// The issued bearer token.
    pub token: String,
    /// When the token expires (epoch seconds).
    pub expires_at: i64,
}

/// Rebuilds a [`PublicKeySet`] from a stored account row.
#[must_use]
pub fn account_keyset(account: &account::Model) -> PublicKeySet {
    PublicKeySet {
        ed25519: account.ed25519_pk.clone(),
        ml_dsa_65: account.ml_dsa_65_pk.clone(),
    }
}

/// `POST /api/v1/auth/register` — creates a crypto-bound account.
pub async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> ApiResult<Json<RegisterResponse>> {
    if request.username.trim().is_empty() {
        return Err(ApiError::BadRequest("username must not be empty".into()));
    }
    let keyset = PublicKeySet {
        ed25519: from_hex("ed25519", &request.ed25519)?,
        ml_dsa_65: from_hex("ml_dsa_65", &request.ml_dsa_65)?,
    };
    if keyset.ed25519.len() != 32 {
        return Err(ApiError::BadRequest("ed25519 key must be 32 bytes".into()));
    }
    if keyset.ml_dsa_65.len() != 1952 {
        return Err(ApiError::BadRequest(
            "ml_dsa_65 key must be 1952 bytes".into(),
        ));
    }
    let usr_id = derive_user_id(&keyset)?.0;

    if state
        .storage
        .account_by_username(&request.username)
        .await?
        .is_some()
    {
        return Err(ApiError::Conflict("username is already taken".into()));
    }
    if state.storage.account_by_usr_id(&usr_id).await?.is_some() {
        return Err(ApiError::Conflict("this key is already registered".into()));
    }

    let model = account::ActiveModel {
        usr_id: Set(usr_id.clone()),
        username: Set(request.username.clone()),
        display_name: Set(request.display_name),
        ed25519_pk: Set(keyset.ed25519),
        ml_dsa_65_pk: Set(keyset.ml_dsa_65),
        created_at: Set(now_epoch()),
    };
    state.storage.create_account(model).await?;
    tracing::info!(%usr_id, username = %request.username, "registered account");
    Ok(Json(RegisterResponse { usr_id }))
}

/// `POST /api/v1/auth/challenge` — issues a single-use login nonce.
pub async fn challenge(
    State(state): State<AppState>,
    Json(request): Json<ChallengeRequest>,
) -> ApiResult<Json<ChallengeResponse>> {
    let account = state
        .storage
        .account_by_username(&request.username)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    let nonce = random_token();
    let now = now_epoch();
    let model = challenge::ActiveModel {
        nonce: Set(nonce.clone()),
        usr_id: Set(account.usr_id.clone()),
        created_at: Set(now),
        expires_at: Set(now + state.config.challenge_ttl_secs),
    };
    state.storage.create_challenge(model).await?;
    tracing::debug!(usr_id = %account.usr_id, "issued login challenge");

    Ok(Json(ChallengeResponse {
        message: login_message(&nonce),
        usr_id: account.usr_id,
        nonce,
    }))
}

/// `POST /api/v1/auth/login` — verifies a signed challenge and issues a token.
pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> ApiResult<Json<LoginResponse>> {
    let challenge = state
        .storage
        .take_challenge(&request.nonce)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if challenge.expires_at <= now_epoch() {
        return Err(ApiError::Unauthorized);
    }
    let account = state
        .storage
        .account_by_usr_id(&challenge.usr_id)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    let signature = from_hex("signature", &request.signature)?;
    let message = login_message(&request.nonce);
    verify_composite(&account_keyset(&account), message.as_bytes(), &signature)
        .map_err(|_| ApiError::Unauthorized)?;

    let token = random_token();
    let now = now_epoch();
    let expires_at = now + state.config.session_ttl_secs;
    let model = session::ActiveModel {
        token: Set(token.clone()),
        usr_id: Set(account.usr_id.clone()),
        created_at: Set(now),
        expires_at: Set(expires_at),
    };
    state.storage.create_session(model).await?;
    tracing::info!(usr_id = %account.usr_id, "login succeeded");

    Ok(Json(LoginResponse {
        usr_id: account.usr_id,
        token,
        expires_at,
    }))
}

/// `POST /api/v1/auth/logout` — ends the caller's session.
pub async fn logout(
    State(state): State<AppState>,
    account: AuthAccount,
) -> ApiResult<Json<serde_json::Value>> {
    state.storage.delete_session(&account.token).await?;
    tracing::info!(usr_id = %account.account.usr_id, "logout");
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// An authenticated account, extracted from a `Bearer` session token.
///
/// Used as a handler argument; its [`FromRequestParts`] implementation
/// rejects the request with `401 Unauthorized` if the token is missing,
/// unknown, or expired.
#[derive(Debug, Clone)]
pub struct AuthAccount {
    /// The authenticated account record.
    pub account: account::Model,
    /// The bearer token that authenticated this request.
    pub token: String,
}

impl FromRequestParts<AppState> for AuthAccount {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::to_owned)
            .ok_or(ApiError::Unauthorized)?;

        let session = state
            .storage
            .valid_session(&token)
            .await?
            .ok_or(ApiError::Unauthorized)?;
        let account = state
            .storage
            .account_by_usr_id(&session.usr_id)
            .await?
            .ok_or(ApiError::Unauthorized)?;
        Ok(Self { account, token })
    }
}
