//! # crisp-server
//!
//! The Crisp backend. It is a thin application layer over [`crisp_core`]: the
//! HTTP surface, a PostgreSQL store, crypto-bound accounts, bundle publication
//! with server endorsement, full-text search, and version negotiation. Every
//! format detail — canonical CBOR, identity derivation, signatures, the A–E
//! verification pipeline — lives in `crisp-core` and is *never* re-implemented
//! here, so the server and client cannot drift in format semantics.
//!
//! The HTTP layer is [`kynos`]: every operation, body, and error is described
//! by the OpenAPI 3.2 document committed at `spec/openapi.json`.
//!
//! ## Module map
//!
//! - [`config`] — process configuration loaded from the environment.
//! - [`db`] — SeaORM entities and the schema migration.
//! - [`storage`] — the mockable repository trait and its PostgreSQL backing.
//! - [`index`] — pure extraction of search metadata from a bundle payload.
//! - [`platform`] — the server's endorsement signing key.
//! - [`error`] — the API error kinds and their RFC 9457 projection.
//! - [`auth`] — crypto-bound account registration, challenge, and login.
//! - [`bundles`] — bundle publication, endorsement, and retrieval.
//! - [`search`] — full-text and faceted recipe search.
//! - [`version`] — liveness, version negotiation, and version headers.
//! - [`purge`] — the background purge of expired sessions and challenges.
//! - [`state`], [`routes`] — the application context and the router.
//! - [`util`] — wall-clock time and random token generation.

pub mod auth;
pub mod bundles;
pub mod config;
pub mod db;
pub mod error;
pub mod index;
pub mod platform;
pub mod purge;
pub mod routes;
pub mod search;
pub mod state;
pub mod storage;
pub mod util;
pub mod version;

pub use routes::router;

use std::sync::Arc;
use std::time::Duration;

use kynos::server::Server;
use kynos::server::shutdown::Shutdown;
use sea_orm_migration::MigratorTrait;

use crate::config::Config;
use crate::platform::PlatformKey;
use crate::purge::Purger;
use crate::state::App;
use crate::storage::{PgStorage, Storage};

/// How long in-flight requests may drain after a shutdown signal.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(20);

/// Boots the server: loads configuration, connects to PostgreSQL, applies
/// pending migrations, starts the purge task, and serves the API until a
/// shutdown signal (SIGINT/SIGTERM) arrives.
///
/// # Errors
///
/// Returns an error if configuration, the database connection, the migration
/// run, the platform key, router construction, or the network bind fails.
pub async fn run() -> eyre::Result<()> {
    let config = Arc::new(Config::from_env()?);
    tracing::info!(bind = %config.bind_addr, "starting crisp-server");

    let db = sea_orm::Database::connect(&config.database_url).await?;
    tracing::info!("connected to PostgreSQL; applying migrations");
    db::Migrator::up(&db, None).await?;

    let platform = Arc::new(PlatformKey::from_seed(&config.platform_seed)?);
    tracing::info!(usr_id = %platform.user_id(), "platform endorsement key ready");

    let storage: Arc<dyn Storage> = Arc::new(PgStorage::new(db));
    let service = router(&config.http).build(App::new(
        Arc::clone(&storage),
        platform,
        Arc::clone(&config),
    ))?;

    let purger = Purger::spawn(
        Arc::clone(&storage),
        Duration::from_secs(config.purge_interval_secs),
    );

    let served = Server::new(service)
        .bind(config.bind_addr.as_str())
        .graceful_shutdown(Shutdown::signals())
        .shutdown_timeout(SHUTDOWN_TIMEOUT)
        .serve()
        .await;

    purger.shutdown().await;
    match served {
        Err(kynos::Error::Server(error)) if error.is_requested_shutdown() => {
            tracing::info!(%error, "server shut down");
            Ok(())
        }
        Ok(()) => {
            tracing::info!("server shut down");
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}
