//! Crypto-bound accounts: registration, login challenge, login, logout, and
//! bearer-token authentication (plan Stage 12).
//!
//! An account *is* a Composite ML-DSA key set. The client derives that key
//! deterministically from `(username, passphrase)` via Argon2id (in
//! `crisp-core`); the passphrase never leaves the client. Registration stores
//! the username and the **public** key set — whose hash is the `usr…` id.
//! Login is a signed-challenge handshake: the server hands out a nonce, the
//! client signs it with the secret key, and the server verifies the signature
//! against the stored public key set. Possessing the account therefore means
//! possessing the key, which means knowing the credentials.

use crisp_core::identity::derive_user_id;
use crisp_core::model::meta::PublicKeySet;
use crisp_core::model::profile::AccountProfile;
use crisp_core::profile::verify_profile;
use crisp_core::sign::{login_message, verify_composite};
use kynos::error::rejection::AuthRejection;
use kynos::extract::body::binary::Binary;
use kynos::http::media::OctetStream;
use kynos::prelude::*;
use kynos::security::carrier::BearerToken;
use kynos::security::{Authenticates, Authenticator, Unscoped};
use sea_orm::ActiveValue::Set;
use serde::{Deserialize, Serialize};

use crate::db::{self, account, session};
use crate::error::{BadRequest, Conflict, NotFound, Unauthorized};
use crate::operation_error;
use crate::state::{App, Platform, Settings, Store};
use crate::storage::InsertError;
use crate::util::{now_epoch, random_token};

/// Decodes a hex string into bytes, mapping failure to a 400.
fn from_hex(label: &str, value: &str) -> Result<Vec<u8>, BadRequest> {
    hex::decode(value.trim()).map_err(|_| BadRequest::new(format!("{label} is not valid hex")))
}

/// Verifies a client's answer to a login challenge: a composite signature
/// over `crisp_core::sign::login_message(audience, nonce)`, where `audience`
/// is this server's platform `usr…` id (spec §8.7).
///
/// # Errors
///
/// Returns an error if the nonce is not hex or the signature does not verify
/// under `keyset`.
pub fn verify_login_signature(
    keyset: &PublicKeySet,
    audience: &str,
    nonce: &str,
    signature: &[u8],
) -> crisp_core::Result<()> {
    let nonce = hex::decode(nonce)
        .map_err(|_| crisp_core::CrispError::input("the challenge nonce is not hex"))?;
    verify_composite(keyset, &login_message(audience, &nonce)?, signature)
}

// ---------------------------------------------------------------------------
// Bearer-token security scheme
// ---------------------------------------------------------------------------

/// An authenticated caller: the account a valid session token resolves to.
#[derive(Debug, Clone)]
pub struct AuthAccount {
    /// The authenticated account record.
    pub account: account::Model,
    /// The bearer token that authenticated this request.
    pub token: String,
}

/// The opaque session token issued by `POST /auth/login`, presented as
/// `Authorization: Bearer <token>`.
#[derive(SecurityScheme)]
#[security(bearer)]
#[security(
    name = "sessionToken",
    credential = AuthAccount,
    description = "An opaque session token issued by `POST /api/v1/auth/login`."
)]
pub struct SessionToken;

/// Resolves [`SessionToken`]s against storage.
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionAuthenticator;

impl Authenticator<SessionToken, App> for SessionAuthenticator {
    async fn authenticate(
        &self,
        presented: BearerToken,
        context: &App,
    ) -> Result<AuthAccount, AuthRejection> {
        let token = presented.into_inner();
        // A storage failure cannot be reported as a 500 from here (an
        // authenticator answers 401/403 only), so it is logged loudly and the
        // caller is refused.
        let session = match context.storage.valid_session(&token).await {
            Ok(Some(session)) => session,
            Ok(None) => {
                tracing::debug!("bearer token unknown or expired");
                return Err(AuthRejection::unauthenticated());
            }
            Err(error) => {
                tracing::error!(%error, "session lookup failed during authentication");
                return Err(AuthRejection::unauthenticated());
            }
        };
        match context.storage.account_by_usr_id(&session.usr_id).await {
            Ok(Some(account)) => {
                tracing::trace!(usr_id = %account.usr_id, "bearer token authenticated");
                Ok(AuthAccount { account, token })
            }
            Ok(None) => {
                tracing::warn!(usr_id = %session.usr_id, "session refers to a missing account");
                Err(AuthRejection::unauthenticated())
            }
            Err(error) => {
                tracing::error!(%error, "account lookup failed during authentication");
                Err(AuthRejection::unauthenticated())
            }
        }
    }

    async fn authorize(
        &self,
        _credential: &AuthAccount,
        _scopes: &[Unscoped],
        _context: &App,
    ) -> Result<(), AuthRejection> {
        // Crisp has no scopes: any authenticated account may act as itself.
        Ok(())
    }
}

impl Authenticates<SessionToken> for App {
    type Authenticator = SessionAuthenticator;

    fn authenticator(&self) -> &Self::Authenticator {
        &self.sessions
    }
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// The `POST /api/v1/auth/register` request body.
///
/// Registration carries a single self-signed account profile: the username,
/// display name, and public key set all live inside it, signed by the very
/// key they describe. The server trusts none of those fields until the
/// signature verifies, so a display name is self-asserted rather than
/// server-trusted.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct RegisterRequest {
    /// The hex-encoded `signed-profile` CBOR (see `crisp-core`'s `profile`).
    pub profile: String,
}

/// The `POST /api/v1/auth/register` response body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct RegisterResponse {
    /// The derived `usr…` account id.
    pub usr_id: String,
}

/// The `POST /api/v1/auth/challenge` request body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct ChallengeRequest {
    /// The username to authenticate.
    pub username: String,
}

/// The `POST /api/v1/auth/challenge` response body.
///
/// Deliberately identical in shape for known and unknown usernames, so the
/// endpoint cannot be used to enumerate accounts.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct ChallengeResponse {
    /// The single-use challenge nonce, hex-encoded.
    pub nonce: String,
    /// The platform `usr…` id the answer must be bound to (spec §8.7).
    pub audience: String,
}

/// The `POST /api/v1/auth/login` request body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct LoginRequest {
    /// The nonce from the matching challenge.
    pub nonce: String,
    /// The composite signature over the challenge message, hex-encoded.
    pub signature: String,
}

/// The `POST /api/v1/auth/login` response body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct LoginResponse {
    /// The authenticated `usr…` id.
    pub usr_id: String,
    /// The issued bearer token.
    pub token: String,
    /// When the token expires (epoch seconds).
    pub expires_at: i64,
}

/// What `GET /accounts/{usr_id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct AccountPath {
    /// The `usr…` account id.
    pub usr_id: String,
}

operation_error! {
    /// What `POST /auth/register` can fail with.
    pub enum RegisterError { BadRequest, Conflict }
}

operation_error! {
    /// What `GET /accounts/{usr_id}` can fail with.
    pub enum AccountError { NotFound }
}

operation_error! {
    /// What `POST /auth/challenge` can fail with.
    pub enum ChallengeError {}
}

operation_error! {
    /// What `POST /auth/login` can fail with.
    pub enum LoginError { BadRequest, Unauthorized }
}

operation_error! {
    /// What `POST /auth/logout` can fail with.
    pub enum LogoutError {}
}

/// Rebuilds a [`PublicKeySet`] from a stored account row.
#[must_use]
pub fn account_keyset(account: &account::Model) -> PublicKeySet {
    PublicKeySet {
        ed25519: account.ed25519_pk.clone(),
        ml_dsa_65: account.ml_dsa_65_pk.clone(),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Creates a crypto-bound account.
///
/// The submitted profile's signature is verified before any field is trusted;
/// the `usr…` id is then derived from the profile's key set. Uniqueness is
/// enforced on both the username and the key: a duplicate key and a duplicate
/// username are distinct conflicts, so the client can tell "this is already
/// your identity" apart from "name taken".
#[kynos::post("/auth/register")]
#[tracing::instrument(level = "info", skip_all)]
pub async fn register(
    Inject(storage): Inject<Store>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, RegisterError> {
    let signed_profile = from_hex("profile", &request.profile)?;
    let AccountProfile {
        keyset,
        username,
        display_name,
        ..
    } = verify_profile(&signed_profile).map_err(|e| {
        tracing::info!(error = %e, "register rejected: invalid profile");
        BadRequest::new(format!("invalid account profile: {e}"))
    })?;
    let usr_id = derive_user_id(&keyset).map_err(BadRequest::from)?.0;

    // The key is checked first: a duplicate key means the caller re-derived an
    // identity that already exists (a returning user), which the client uses
    // to fall through to sign-in — so it must win over a username clash.
    if storage.account_by_usr_id(&usr_id).await?.is_some() {
        tracing::info!(%usr_id, "register rejected: key already registered");
        return Err(Conflict::new("this key is already registered").into());
    }
    if storage.account_by_username(&username).await?.is_some() {
        tracing::info!(%username, "register rejected: username taken");
        return Err(Conflict::new("username is already taken").into());
    }

    let model = account::ActiveModel {
        usr_id: Set(usr_id.clone()),
        username: Set(username.clone()),
        display_name: Set(display_name),
        ed25519_pk: Set(keyset.ed25519),
        ml_dsa_65_pk: Set(keyset.ml_dsa_65),
        signed_profile: Set(signed_profile),
        created_at: Set(now_epoch()),
    };
    match storage.create_account(model).await {
        Ok(()) => {}
        // A concurrent registration won the race between the checks above
        // and this insert; the unique constraint is the real arbiter.
        Err(InsertError::Duplicate(detail)) => {
            tracing::info!(%usr_id, %username, %detail, "register rejected: concurrent duplicate");
            return Err(Conflict::new("this key or username is already registered").into());
        }
        Err(InsertError::Db(error)) => return Err(error.into()),
    }
    tracing::info!(%usr_id, %username, "registered account");
    Ok(Json(RegisterResponse { usr_id }))
}

/// Returns an account's self-signed profile.
///
/// The bytes are a `signed-profile` (canonical CBOR); any client can
/// re-verify them with `crisp-core`'s `verify_profile`, so the display name
/// stays self-asserted even when read back from the server.
#[kynos::get("/accounts/{usr_id}")]
#[tracing::instrument(level = "debug", skip(storage))]
pub async fn account_profile(
    Inject(storage): Inject<Store>,
    Path(path): Path<AccountPath>,
) -> Result<Binary<OctetStream>, AccountError> {
    let account = storage
        .account_by_usr_id(&path.usr_id)
        .await?
        .ok_or_else(|| NotFound::new("no account with that id"))?;
    Ok(Binary::new(account.signed_profile))
}

/// Issues a single-use login nonce.
///
/// Always answers 200 with a fresh nonce, whether or not the username exists,
/// so the endpoint cannot be used to discover accounts. For an unknown
/// username nothing is stored, so its nonce can never be redeemed.
#[kynos::post("/auth/challenge")]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn challenge(
    Inject(storage): Inject<Store>,
    Inject(settings): Inject<Settings>,
    Inject(platform): Inject<Platform>,
    Json(request): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, ChallengeError> {
    let nonce = random_token();
    if let Some(account) = storage.account_by_username(&request.username).await? {
        let now = now_epoch();
        let model = db::challenge::ActiveModel {
            nonce: Set(nonce.clone()),
            usr_id: Set(account.usr_id.clone()),
            created_at: Set(now),
            expires_at: Set(now + settings.challenge_ttl_secs),
        };
        storage.create_challenge(model).await?;
        tracing::debug!(usr_id = %account.usr_id, "issued login challenge");
    } else {
        tracing::debug!("issued decoy challenge for an unknown username");
    }

    Ok(Json(ChallengeResponse {
        nonce,
        audience: platform.user_id().to_owned(),
    }))
}

/// Verifies a signed challenge and issues a session token.
///
/// The nonce is consumed atomically before anything else, so it can be
/// redeemed at most once — a replayed or concurrent second login fails.
#[kynos::post("/auth/login")]
#[tracing::instrument(level = "info", skip_all)]
pub async fn login(
    Inject(storage): Inject<Store>,
    Inject(settings): Inject<Settings>,
    Inject(platform): Inject<Platform>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, LoginError> {
    let refused = || Unauthorized::new("the challenge is unknown, expired, or was not answered");

    let Some(challenge) = storage.take_challenge(&request.nonce).await? else {
        tracing::info!("login rejected: unknown or already-used nonce");
        return Err(refused().into());
    };
    if challenge.expires_at <= now_epoch() {
        tracing::info!(usr_id = %challenge.usr_id, "login rejected: challenge expired");
        return Err(refused().into());
    }
    let Some(account) = storage.account_by_usr_id(&challenge.usr_id).await? else {
        tracing::warn!(usr_id = %challenge.usr_id, "login rejected: challenge for missing account");
        return Err(refused().into());
    };

    let signature = from_hex("signature", &request.signature)?;
    let audience = platform.user_id();
    if verify_login_signature(
        &account_keyset(&account),
        audience,
        &request.nonce,
        &signature,
    )
    .is_err()
    {
        tracing::info!(usr_id = %account.usr_id, "login rejected: bad signature");
        return Err(refused().into());
    }

    let token = random_token();
    let now = now_epoch();
    let expires_at = now + settings.session_ttl_secs;
    let model = session::ActiveModel {
        token: Set(token.clone()),
        usr_id: Set(account.usr_id.clone()),
        created_at: Set(now),
        expires_at: Set(expires_at),
    };
    storage.create_session(model).await?;
    tracing::info!(usr_id = %account.usr_id, expires_at, "login succeeded");

    Ok(Json(LoginResponse {
        usr_id: account.usr_id,
        token,
        expires_at,
    }))
}

/// Ends the caller's session; the token stops authenticating immediately.
#[kynos::post("/auth/logout")]
#[tracing::instrument(level = "info", skip_all)]
pub async fn logout(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
) -> Result<NoContent, LogoutError> {
    let caller = auth.into_inner();
    storage.delete_session(&caller.token).await?;
    tracing::info!(usr_id = %caller.account.usr_id, "logout");
    Ok(NoContent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_signature_round_trips_and_is_bound_to_its_audience() {
        let key = crisp_core::sign::SecretKeySet::from_seed(&[9u8; 32]);
        let message = login_message("usr1server", &[0xab, 0xcd]).unwrap();
        let signature = key.sign(&message).unwrap();
        let keyset = key.public_keyset();
        assert!(verify_login_signature(&keyset, "usr1server", "abcd", &signature).is_ok());
        assert!(verify_login_signature(&keyset, "usr1other", "abcd", &signature).is_err());
        assert!(verify_login_signature(&keyset, "usr1server", "abce", &signature).is_err());
        assert!(verify_login_signature(&keyset, "usr1server", "zz", &signature).is_err());
    }

    #[test]
    fn from_hex_rejects_garbage() {
        assert!(from_hex("x", "zz").is_err());
        assert_eq!(from_hex("x", " 0a ").unwrap(), vec![10]);
    }
}
