//! Deleting the caller's own account (`docs/public-server-policy.md` §3.2).
//!
//! `DELETE /account` removes the account, its sessions and login
//! challenges, and every bundle it published, in one transaction. It needs a
//! recent sign-in, so a stolen long-lived token cannot erase an account: the
//! session must be at most [`REAUTH_WINDOW_SECS`] old, which the client
//! satisfies by signing in again with its key file. The username is free
//! again as soon as the deletion commits.

use kynos::prelude::*;

use super::SessionToken;
use crate::error::ReauthenticationRequired;
use crate::operation_error;
use crate::state::Store;
use crate::util::now_epoch;

/// The oldest a session may be, in seconds since it was issued, to delete
/// its account: ten minutes.
pub const REAUTH_WINDOW_SECS: i64 = 10 * 60;

operation_error! {
    /// What `DELETE /account` can fail with.
    pub enum DeleteAccountError { ReauthenticationRequired }
}

/// Whether a session issued at `issued_at` is recent enough at `now` to
/// delete its account. A session issued in the future (clock skew) is
/// recent.
#[must_use]
pub const fn signed_in_recently(issued_at: i64, now: i64) -> bool {
    now.saturating_sub(issued_at) <= REAUTH_WINDOW_SECS
}

/// Deletes the caller's account, its sessions and login challenges, and
/// every bundle it published; co-authors' revisions of the same documents
/// stay.
///
/// Refused 403 `reauthentication_required` when the session is older than
/// [`REAUTH_WINDOW_SECS`]: sign in again and retry with the new session.
#[kynos::delete("/account")]
#[tracing::instrument(level = "info", skip_all, fields(usr_id))]
pub async fn delete_account(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
) -> Result<NoContent, DeleteAccountError> {
    let caller = auth.into_inner().resolve()?;
    let usr_id = caller.account.usr_id;
    tracing::Span::current().record("usr_id", usr_id.as_str());
    let now = now_epoch();
    if !signed_in_recently(caller.issued_at, now) {
        let age = now.saturating_sub(caller.issued_at);
        tracing::info!(age, "account deletion refused: session too old");
        return Err(ReauthenticationRequired::new(REAUTH_WINDOW_SECS).into());
    }
    match storage.delete_account(&usr_id).await? {
        Some(removed) => tracing::info!(
            username = %caller.account.username,
            bundles = removed.bundles,
            bytes = removed.bytes,
            "account deleted"
        ),
        // A concurrent deletion won; the account is gone either way.
        None => tracing::info!("account already deleted"),
    }
    Ok(NoContent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_is_recent_up_to_the_window_and_not_after() {
        assert!(signed_in_recently(1_000, 1_000));
        assert!(signed_in_recently(1_000, 1_000 + REAUTH_WINDOW_SECS));
        assert!(!signed_in_recently(1_000, 1_001 + REAUTH_WINDOW_SECS));
        assert!(
            signed_in_recently(2_000, 1_000),
            "clock skew reads as recent"
        );
        assert!(!signed_in_recently(i64::MIN, i64::MAX));
    }

    #[test]
    fn the_reauthentication_window_is_ten_minutes() {
        assert!(signed_in_recently(0, 600));
        assert!(!signed_in_recently(0, 601));
    }
}
