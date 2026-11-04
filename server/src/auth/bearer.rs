//! The bearer-token security scheme: a session token issued by
//! `POST /auth/login`, resolved to its account on every protected request.

use kynos::error::rejection::AuthRejection;
use kynos::prelude::*;
use kynos::security::carrier::BearerToken;
use kynos::security::{Authenticator, Unscoped};

use sea_orm::DbErr;

use crate::db::account;
use crate::error::DbFailure;
use crate::state::App;

/// An authenticated caller: the account a valid session token resolves to.
#[derive(Debug, Clone)]
pub struct AuthAccount {
    /// The authenticated account record.
    pub account: account::Model,
    /// The bearer token that authenticated this request.
    pub token: String,
}

/// What the authenticator made of a presented bearer token: the caller's
/// account, or the database failure that kept it from deciding.
///
/// kynos lets an authenticator refuse only with 401 or 403, and a database
/// outage is neither: answering it 401 would tell every signed-in client its
/// session is invalid. So the authenticator refuses (401) only a token it
/// positively found unknown, expired, or orphaned; when the lookup itself
/// fails it lets the request through carrying the failure, and the handler
/// answers it through [`SessionLookup::resolve`] — 503 for an outage, 500
/// otherwise.
#[derive(Debug)]
pub struct SessionLookup(Result<AuthAccount, DbFailure>);

impl SessionLookup {
    /// The authenticated caller.
    ///
    /// # Errors
    ///
    /// Returns the [`DbFailure`] that interrupted the session lookup.
    pub fn resolve(self) -> Result<AuthAccount, DbFailure> {
        self.0
    }
}

/// The opaque session token issued by `POST /auth/login`, presented as
/// `Authorization: Bearer <token>`.
#[derive(SecurityScheme)]
#[security(bearer)]
#[security(
    name = "sessionToken",
    credential = SessionLookup,
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
    ) -> Result<SessionLookup, AuthRejection> {
        let token = presented.into_inner();
        let deferred = |what: &str, error: DbErr| {
            // `DbFailure::from` logs the error itself at ERROR.
            tracing::debug!(what, "lookup failed during authentication; deferring");
            Ok(SessionLookup(Err(DbFailure::from(error))))
        };
        let session = match context.storage.valid_session(&token).await {
            Ok(Some(session)) => session,
            Ok(None) => {
                tracing::debug!("bearer token unknown or expired");
                return Err(AuthRejection::unauthenticated());
            }
            Err(error) => return deferred("session", error),
        };
        match context.storage.account_by_usr_id(&session.usr_id).await {
            Ok(Some(account)) => {
                tracing::trace!(usr_id = %account.usr_id, "bearer token authenticated");
                Ok(SessionLookup(Ok(AuthAccount { account, token })))
            }
            Ok(None) => {
                tracing::warn!(usr_id = %session.usr_id, "session refers to a missing account");
                Err(AuthRejection::unauthenticated())
            }
            Err(error) => deferred("account", error),
        }
    }

    async fn authorize(
        &self,
        _credential: &SessionLookup,
        _scopes: &[Unscoped],
        _context: &App,
    ) -> Result<(), AuthRejection> {
        // Crisp has no scopes: any authenticated account may act as itself.
        Ok(())
    }
}
