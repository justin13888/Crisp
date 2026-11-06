use super::*;
use sea_orm::{RuntimeErr, SqlxError};

#[test]
fn connection_failures_are_unavailable_and_statement_failures_are_not() {
    use sea_orm::ConnAcquireErr;
    let sqlx = |error: SqlxError| RuntimeErr::SqlxError(std::sync::Arc::new(error));
    let io = || SqlxError::Io(std::io::Error::other("connection reset"));
    let unavailable = [
        DbErr::ConnectionAcquire(ConnAcquireErr::Timeout),
        DbErr::ConnectionAcquire(ConnAcquireErr::ConnectionClosed),
        DbErr::Conn(RuntimeErr::Internal("refused".to_owned())),
        DbErr::Exec(sqlx(io())),
        DbErr::Query(sqlx(io())),
        DbErr::Query(sqlx(SqlxError::PoolTimedOut)),
        DbErr::Query(sqlx(SqlxError::Protocol("eof".to_owned()))),
    ];
    for error in &unavailable {
        assert!(is_unavailable(error), "{error} must be unavailable");
    }
    let faulty = [
        DbErr::Custom("boom".to_owned()),
        DbErr::RecordNotFound("x".to_owned()),
        DbErr::Query(RuntimeErr::Internal("bad sql".to_owned())),
        DbErr::Query(sqlx(SqlxError::RowNotFound)),
        DbErr::Exec(sqlx(SqlxError::ColumnNotFound("x".to_owned()))),
    ];
    for error in &faulty {
        assert!(!is_unavailable(error), "{error} must not be unavailable");
    }
}

#[test]
fn session_digest_is_hex_sha256_and_never_the_token() {
    assert_eq!(
        session_digest("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let token = crate::util::random_token();
    let digest = session_digest(&token);
    assert_ne!(digest, token);
    assert_eq!(digest.len(), 64);
    assert_ne!(session_digest(&digest), digest);
}
