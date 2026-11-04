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
//! - [`limits`] — the per-operation rate-limit budgets and their counters.
//! - [`platform`] — the server's endorsement signing key.
//! - [`error`] — the API error kinds and their RFC 9457 projection.
//! - [`auth`] — crypto-bound account registration, challenge, and login.
//! - [`bundles`] — bundle publication, endorsement, and retrieval.
//! - [`quota`] — per-account storage quotas, the server cap, and usage.
//! - [`moderation`] — reports, takedowns, suspension, and their audit trail.
//! - [`search`] — full-text and faceted recipe search.
//! - [`version`] — liveness, version negotiation, and version headers.
//! - [`purge`] — the background purge of expired sessions and challenges.
//! - [`metrics`] — the Prometheus metrics and their separate listener.
//! - [`state`], [`routes`] — the application context and the router.
//! - [`util`] — wall-clock time and random token generation.

pub mod auth;
pub mod bundles;
pub mod config;
pub mod db;
pub mod error;
pub mod index;
pub mod limits;
pub mod metrics;
pub mod moderation;
pub mod platform;
pub mod purge;
pub mod quota;
pub mod routes;
pub mod search;
pub mod state;
pub mod storage;
pub mod util;
pub mod version;

pub use routes::router;

use std::sync::Arc;
use std::time::Duration;

use kynos::server::shutdown::Shutdown;
use sea_orm_migration::MigratorTrait;

use crate::config::Config;
use crate::metrics::exporter::{Exporter, MetricsListener, Pool, PoolProbe};
use crate::platform::PlatformKey;
use crate::purge::Purger;
use crate::state::App;
use crate::storage::{PgStorage, Storage};

/// How long in-flight requests may drain after a shutdown signal.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(20);

/// Boots the server: loads configuration, connects to PostgreSQL, applies
/// pending migrations, starts the metrics listener (unless
/// `CRISP_METRICS_BIND_ADDR=off`) and the purge task, and serves the API
/// until a shutdown signal (SIGINT/SIGTERM) arrives.
///
/// Boot does not retry an unreachable database: it fails fast and exits
/// non-zero, and the supervisor's restart policy (`restart: unless-stopped`
/// in `compose.prod.yaml`) provides the retry and backoff. Once serving, a
/// lost database answers 503 and fails the readiness probe until it returns.
///
/// # Errors
///
/// Returns an error if configuration, the database connection, the migration
/// run, the platform key, router construction, or either network bind (the
/// API's or the metrics listener's) fails.
pub async fn run() -> eyre::Result<()> {
    let config = Arc::new(Config::from_env()?);
    tracing::info!(bind = %config.bind_addr, "starting crisp-server");
    config.moderation.log_startup();

    let pool = &config.database;
    tracing::info!(
        max_connections = pool.max_connections,
        acquire_timeout_secs = pool.acquire_timeout.as_secs(),
        statement_timeout_secs = pool.statement_timeout.as_secs(),
        "connecting to PostgreSQL"
    );
    let db = sea_orm::Database::connect(pool.connect_options(&config.database_url)).await?;
    tracing::info!("connected to PostgreSQL; applying migrations");
    db::Migrator::up(&db, None).await?;

    let platform = Arc::new(PlatformKey::from_seed(&config.platform_seed)?);
    tracing::info!(usr_id = %platform.user_id(), "platform endorsement key ready");

    let pool: Arc<dyn PoolProbe> = Arc::new(db.get_postgres_connection_pool().clone());
    let storage: Arc<dyn Storage> = Arc::new(PgStorage::new(db));
    let app = App::new(Arc::clone(&storage), platform, Arc::clone(&config));
    let metrics = app.metrics.clone();
    let service = router(&config.http)
        .observe(metrics.observer())
        .build(app)?;

    let exporter = match &config.observability.metrics_bind_addr {
        Some(address) => {
            let exporter = Exporter {
                metrics: metrics.clone(),
                pool: Pool(Some(pool)),
            };
            Some(MetricsListener::start(address, exporter).await?)
        }
        None => {
            tracing::info!("metrics listener disabled (CRISP_METRICS_BIND_ADDR=off)");
            None
        }
    };

    let purger = Purger::spawn(
        Arc::clone(&storage),
        Duration::from_secs(config.purge_interval_secs),
        metrics,
    );

    tracing::info!(trusted_proxies = %config.http.proxies.policy(), "client addresses");
    let served = routes::server(service, &config.http.connections)
        .bind(config.bind_addr.as_str())
        .graceful_shutdown(Shutdown::signals())
        .shutdown_timeout(SHUTDOWN_TIMEOUT)
        .serve()
        .await;

    purger.shutdown().await;
    if let Some(exporter) = exporter {
        exporter.shutdown().await;
    }
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
