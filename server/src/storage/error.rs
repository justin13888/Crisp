//! Storage failures: the insert errors the schema's constraints and the
//! quotas produce, and which database errors mean "unavailable".

use sea_orm::{DbErr, RuntimeErr, SqlErr, SqlxError};

use crate::quota::Breach;

/// A failure inserting a row whose uniqueness the schema enforces.
#[derive(Debug, thiserror::Error)]
pub enum InsertError {
    /// A unique constraint (or primary key) rejected the row: the record
    /// already exists, possibly inserted by a concurrent request.
    #[error("unique constraint violated: {0}")]
    Duplicate(String),
    /// Any other database failure.
    #[error(transparent)]
    Db(#[from] DbErr),
}

/// A failure storing a bundle: an insert failure, or a storage quota the
/// bundle does not fit.
#[derive(Debug, thiserror::Error)]
pub enum BundleInsertError {
    /// Storing the bundle would breach a storage quota; nothing was written.
    #[error(transparent)]
    Quota(#[from] Breach),
    /// The insert itself failed.
    #[error(transparent)]
    Insert(#[from] InsertError),
}

impl From<DbErr> for BundleInsertError {
    fn from(error: DbErr) -> Self {
        Self::Insert(InsertError::Db(error))
    }
}

impl InsertError {
    /// Classifies a database error, recognizing unique-constraint violations.
    #[must_use]
    pub fn classify(error: DbErr) -> Self {
        match error.sql_err() {
            Some(SqlErr::UniqueConstraintViolation(detail)) => Self::Duplicate(detail),
            _ => Self::Db(error),
        }
    }
}

/// Whether `error` means the database could not be reached or could not
/// serve the statement in time, rather than that the statement was wrong.
///
/// These are the failures a client should retry later (an outage, an
/// exhausted pool, a restart, a statement cut off by `statement_timeout`),
/// so the API answers them 503 instead of 500. Recognized: a pool acquire
/// failure, a connection error, an I/O, TLS, or protocol failure underneath
/// a statement, and the PostgreSQL SQLSTATE classes 08 (connection
/// exception), 53 (insufficient resources), and 57 (operator intervention,
/// which includes `57014 query_canceled` and the shutdown codes).
#[must_use]
pub fn is_unavailable(error: &DbErr) -> bool {
    match error {
        DbErr::ConnectionAcquire(_) | DbErr::Conn(_) => true,
        DbErr::Exec(RuntimeErr::SqlxError(error)) | DbErr::Query(RuntimeErr::SqlxError(error)) => {
            match error {
                SqlxError::Io(_)
                | SqlxError::Tls(_)
                | SqlxError::Protocol(_)
                | SqlxError::PoolTimedOut
                | SqlxError::PoolClosed
                | SqlxError::WorkerCrashed => true,
                SqlxError::Database(error) => error
                    .code()
                    .is_some_and(|code| ["08", "53", "57"].iter().any(|c| code.starts_with(c))),
                _ => false,
            }
        }
        _ => false,
    }
}
