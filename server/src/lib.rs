//! # crisp-server
//!
//! The Crisp backend. It is a thin application layer over [`crisp_core`]: the
//! HTTP surface, a PostgreSQL store, crypto-bound accounts, bundle publication
//! with server endorsement, full-text search, and version negotiation. Every
//! format detail — canonical CBOR, identity derivation, signatures, the A–E
//! verification pipeline — lives in `crisp-core` and is *never* re-implemented
//! here, so the server and client cannot drift in format semantics.
//!
//! ## Module map
//!
//! - [`config`] — process configuration loaded from the environment.
//! - [`db`] — SeaORM entities and the schema migration.
//! - [`storage`] — the mockable repository trait and its PostgreSQL backing.
//! - [`index`] — pure extraction of search metadata from a bundle payload.
//! - [`platform`] — the server's endorsement signing key.
//! - [`error`] — the API error type and its HTTP projection.
//! - [`auth`] — crypto-bound account registration, challenge, and login.
//! - [`bundles`] — bundle publication, endorsement, and retrieval.
//! - [`search`] — full-text and faceted recipe search.
//! - [`version`] — API/format version negotiation and drift handling.
//! - [`state`], [`routes`] — shared application state and the router.

pub mod auth;
pub mod bundles;
pub mod config;
pub mod db;
pub mod error;
pub mod index;
pub mod platform;
pub mod routes;
pub mod search;
pub mod state;
pub mod storage;
pub mod util;
pub mod version;

pub use error::{ApiError, ApiResult};

use std::sync::Arc;

use sea_orm_migration::MigratorTrait;

use crate::config::Config;
use crate::platform::PlatformKey;
use crate::state::AppState;
use crate::storage::PgStorage;

/// Boots the server: loads configuration, connects to PostgreSQL, applies
/// pending migrations, and serves the API until shutdown.
///
/// # Errors
///
/// Returns an error if configuration, the database connection, the migration
/// run, or the network bind fails.
pub async fn run() -> eyre::Result<()> {
    let config = Config::from_env()?;
    tracing::info!(bind = %config.bind_addr, "starting crisp-server");

    let db = sea_orm::Database::connect(&config.database_url).await?;
    tracing::info!("connected to PostgreSQL; applying migrations");
    db::Migrator::up(&db, None).await?;

    let platform = PlatformKey::from_seed(&config.platform_seed);
    tracing::info!(usr_id = %platform.user_id()?, "platform endorsement key ready");

    let state = AppState {
        storage: Arc::new(PgStorage::new(db)),
        platform: Arc::new(platform),
        config: Arc::new(config.clone()),
    };

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "listening");
    axum::serve(listener, routes::router(state)).await?;
    Ok(())
}
