//! The caller's own sessions: ending the current one, ending all of them,
//! and listing the ones still live.
//!
//! Every handler here is bearer-authenticated and acts only on the sessions
//! of the account the token resolves to. A listed session is identified by a
//! prefix of its stored digest, never by its token, so the list cannot be
//! used to authenticate.

use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use super::SessionToken;
use crate::db::session;
use crate::operation_error;
use crate::state::Store;
use crate::storage::session_digest;

/// How many hex characters of a session's stored digest identify it in a
/// listing: 64 bits, unique among one account's sessions in practice, and
/// far too short to be the digest itself.
pub const SESSION_ID_LEN: usize = 16;

/// One live session, as `GET /auth/sessions` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct SessionInfo {
    /// An opaque, stable identifier: a prefix of the session's stored
    /// digest. It is not the token and does not authenticate.
    pub id: String,
    /// When the session was issued (epoch seconds).
    pub created_at: i64,
    /// When the session expires (epoch seconds).
    pub expires_at: i64,
    /// Whether this is the session that authenticated the request.
    pub current: bool,
}

/// The `GET /auth/sessions` response body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct SessionList {
    /// The caller's unexpired sessions, newest first.
    pub sessions: Vec<SessionInfo>,
}

operation_error! {
    /// What `POST /auth/logout` can fail with.
    pub enum LogoutError {}
}

operation_error! {
    /// What `POST /auth/logout-all` can fail with.
    pub enum LogoutAllError {}
}

operation_error! {
    /// What `GET /auth/sessions` can fail with.
    pub enum SessionsError {}
}

/// Describes a stored session for its owner; `current` is the digest of
/// the token that authenticated the request.
#[must_use]
pub fn session_info(row: &session::Model, current: &str) -> SessionInfo {
    SessionInfo {
        id: row.token.chars().take(SESSION_ID_LEN).collect(),
        created_at: row.created_at,
        expires_at: row.expires_at,
        current: row.token == current,
    }
}

/// Ends the caller's session; the token stops authenticating immediately.
#[kynos::post("/auth/logout")]
#[tracing::instrument(level = "info", skip_all)]
pub async fn logout(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
) -> Result<NoContent, LogoutError> {
    let caller = auth.into_inner().resolve()?;
    storage.delete_session(&caller.token).await?;
    tracing::info!(usr_id = %caller.account.usr_id, "logout");
    Ok(NoContent)
}

/// Ends every session of the caller's account, including the one that
/// authenticated this request: every token it was ever issued stops
/// authenticating immediately, on every device.
#[kynos::post("/auth/logout-all")]
#[tracing::instrument(level = "info", skip_all)]
pub async fn logout_all(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
) -> Result<NoContent, LogoutAllError> {
    let caller = auth.into_inner().resolve()?;
    let usr_id = &caller.account.usr_id;
    let ended = storage.delete_sessions_for(usr_id).await?;
    tracing::info!(%usr_id, ended, "logout everywhere");
    Ok(NoContent)
}

/// Lists the caller's unexpired sessions, newest first, marking the one
/// that authenticated this request.
#[kynos::get("/auth/sessions")]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn sessions(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
) -> Result<Json<SessionList>, SessionsError> {
    let caller = auth.into_inner().resolve()?;
    let rows = storage.sessions_for(&caller.account.usr_id).await?;
    let current = session_digest(&caller.token);
    let sessions: Vec<SessionInfo> = rows.iter().map(|row| session_info(row, &current)).collect();
    tracing::debug!(usr_id = %caller.account.usr_id, count = sessions.len(), "listed sessions");
    Ok(Json(SessionList { sessions }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(token: &str) -> session::Model {
        session::Model {
            token: token.to_owned(),
            usr_id: "usr1a".to_owned(),
            created_at: 10,
            expires_at: 20,
        }
    }

    #[test]
    fn session_info_shows_a_digest_prefix_and_marks_the_current_session() {
        let digest = session_digest("token-a");
        let info = session_info(&row(&digest), &digest);
        assert_eq!(info.id, digest[..SESSION_ID_LEN]);
        assert_eq!((info.created_at, info.expires_at), (10, 20));
        assert!(info.current);
        let other = session_info(&row(&session_digest("token-b")), &digest);
        assert!(!other.current);
        assert_ne!(other.id, info.id);
    }
}
