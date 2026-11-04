use super::*;

#[test]
fn connection_failures_are_unavailable_and_statement_failures_are_not() {
    use sea_orm::ConnAcquireErr;
    let io = || SqlxError::Io(std::io::Error::other("connection reset"));
    let unavailable = [
        DbErr::ConnectionAcquire(ConnAcquireErr::Timeout),
        DbErr::ConnectionAcquire(ConnAcquireErr::ConnectionClosed),
        DbErr::Conn(RuntimeErr::Internal("refused".to_owned())),
        DbErr::Exec(RuntimeErr::SqlxError(io())),
        DbErr::Query(RuntimeErr::SqlxError(io())),
        DbErr::Query(RuntimeErr::SqlxError(SqlxError::PoolTimedOut)),
        DbErr::Query(RuntimeErr::SqlxError(SqlxError::Protocol("eof".to_owned()))),
    ];
    for error in &unavailable {
        assert!(is_unavailable(error), "{error} must be unavailable");
    }
    let faulty = [
        DbErr::Custom("boom".to_owned()),
        DbErr::RecordNotFound("x".to_owned()),
        DbErr::Query(RuntimeErr::Internal("bad sql".to_owned())),
        DbErr::Query(RuntimeErr::SqlxError(SqlxError::RowNotFound)),
        DbErr::Exec(RuntimeErr::SqlxError(SqlxError::ColumnNotFound(
            "x".to_owned(),
        ))),
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
