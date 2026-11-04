//! The SQL behind sessions: issuing, resolving, listing, and ending them.
//! Every row is keyed by [`session_digest`], never by the token itself.

use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder,
};
use sha2::{Digest, Sha256};

use crate::db::session;
use crate::util::now_epoch;

/// The key a session is stored and looked up under: the hex-encoded SHA-256
/// digest of its bearer token.
///
/// The token is 256 bits of randomness, so an unsalted fast hash suffices: a
/// database read yields digests, which cannot be presented as tokens and
/// cannot be inverted to recover one.
#[must_use]
pub fn session_digest(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Stores `session` under the digest of the token it carries.
pub(super) async fn create(
    conn: &impl ConnectionTrait,
    mut session: session::ActiveModel,
) -> Result<(), DbErr> {
    let token = session
        .token
        .take()
        .ok_or_else(|| DbErr::Custom("a new session needs a token".to_owned()))?;
    session.token = Set(session_digest(&token));
    session.insert(conn).await?;
    Ok(())
}

/// The unexpired session for the presented `token`, if any.
pub(super) async fn valid(
    conn: &impl ConnectionTrait,
    token: &str,
) -> Result<Option<session::Model>, DbErr> {
    session::Entity::find_by_id(session_digest(token))
        .filter(session::Column::ExpiresAt.gt(now_epoch()))
        .one(conn)
        .await
}

/// Deletes the session for the presented `token`.
pub(super) async fn delete(conn: &impl ConnectionTrait, token: &str) -> Result<(), DbErr> {
    session::Entity::delete_by_id(session_digest(token))
        .exec(conn)
        .await?;
    Ok(())
}

/// Deletes every session `usr_id` holds, expired or not, in one statement;
/// returns how many there were.
pub(super) async fn delete_for(conn: &impl ConnectionTrait, usr_id: &str) -> Result<u64, DbErr> {
    let deleted = session::Entity::delete_many()
        .filter(session::Column::UsrId.eq(usr_id))
        .exec(conn)
        .await?;
    Ok(deleted.rows_affected)
}

/// The unexpired sessions `usr_id` holds, newest first.
pub(super) async fn live_for(
    conn: &impl ConnectionTrait,
    usr_id: &str,
) -> Result<Vec<session::Model>, DbErr> {
    session::Entity::find()
        .filter(session::Column::UsrId.eq(usr_id))
        .filter(session::Column::ExpiresAt.gt(now_epoch()))
        .order_by_desc(session::Column::CreatedAt)
        .order_by_asc(session::Column::Token)
        .all(conn)
        .await
}
