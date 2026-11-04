//! The PostgreSQL connection-pool settings and the connect options they
//! produce.

use std::time::Duration;

/// The default connection-pool size.
pub const DEFAULT_DB_MAX_CONNECTIONS: u32 = 10;
/// The default wait for a pooled connection before answering 503: 5 seconds.
pub const DEFAULT_DB_ACQUIRE_TIMEOUT_SECS: u64 = 5;
/// The default server-side `statement_timeout`: 30 seconds.
pub const DEFAULT_DB_STATEMENT_TIMEOUT_SECS: u64 = 30;

/// The PostgreSQL connection-pool settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatabaseOptions {
    /// The most connections the pool opens (`CRISP_DB_MAX_CONNECTIONS`).
    pub max_connections: u32,
    /// How long a request waits for a pooled connection, including opening a
    /// new one, before it fails as unavailable (`CRISP_DB_ACQUIRE_TIMEOUT_SECS`).
    pub acquire_timeout: Duration,
    /// The server-side `statement_timeout` every pooled connection sets, so a
    /// runaway statement is cancelled rather than holding its connection
    /// (`CRISP_DB_STATEMENT_TIMEOUT_SECS`).
    pub statement_timeout: Duration,
}

impl Default for DatabaseOptions {
    fn default() -> Self {
        Self {
            max_connections: DEFAULT_DB_MAX_CONNECTIONS,
            acquire_timeout: Duration::from_secs(DEFAULT_DB_ACQUIRE_TIMEOUT_SECS),
            statement_timeout: Duration::from_secs(DEFAULT_DB_STATEMENT_TIMEOUT_SECS),
        }
    }
}

impl DatabaseOptions {
    /// The SeaORM connect options for `url` under these settings.
    ///
    /// The statement timeout travels as a connection startup parameter, so it
    /// holds for every statement on every pooled connection.
    #[must_use]
    pub fn connect_options(&self, url: &str) -> sea_orm::ConnectOptions {
        let statement_timeout_ms = self.statement_timeout.as_millis().to_string();
        let mut options = sea_orm::ConnectOptions::new(url);
        options
            .max_connections(self.max_connections)
            .acquire_timeout(self.acquire_timeout)
            .connect_timeout(self.acquire_timeout)
            .map_sqlx_postgres_opts(move |pg| {
                pg.options([("statement_timeout", statement_timeout_ms.as_str())])
            });
        options
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_options_carry_the_pool_settings() {
        let database = DatabaseOptions {
            max_connections: 4,
            acquire_timeout: Duration::from_secs(2),
            statement_timeout: Duration::from_secs(9),
        };
        let options = database.connect_options("postgresql://localhost/crisp");
        assert_eq!(options.get_max_connections(), Some(4));
        assert_eq!(options.get_acquire_timeout(), Some(Duration::from_secs(2)));
        assert_eq!(options.get_connect_timeout(), Some(Duration::from_secs(2)));
    }
}
