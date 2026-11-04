//! The bearer-token security scheme: a session token issued by
//! `POST /auth/login`, resolved to its account on every protected request.

use kynos::error::rejection::AuthRejection;
use kynos::prelude::*;
use kynos::security::carrier::BearerToken;
use kynos::security::{Authenticates, Authenticator, Unscoped};

use crate::db::account;
use crate::state::App;

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
