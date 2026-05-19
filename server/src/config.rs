//! Process configuration, loaded once from the environment at startup.

use std::env;

use axum::http::HeaderValue;
use eyre::{Context, eyre};

/// The default maximum accepted `.crisp` upload size (16 MiB), matching the
/// spec §12 bundle ceiling.
const DEFAULT_MAX_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
/// The default session lifetime: 30 days.
const DEFAULT_SESSION_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// The default login-challenge lifetime: 5 minutes.
const DEFAULT_CHALLENGE_TTL_SECS: i64 = 5 * 60;

/// All runtime configuration for the server.
#[derive(Debug, Clone)]
pub struct Config {
    /// The PostgreSQL connection URL.
    pub database_url: String,
    /// The `host:port` the HTTP server binds to.
    pub bind_addr: String,
    /// The 32-byte seed for the platform endorsement key.
    pub platform_seed: [u8; 32],
    /// How long an issued session token stays valid, in seconds.
    pub session_ttl_secs: i64,
    /// How long a login challenge stays valid, in seconds.
    pub challenge_ttl_secs: i64,
    /// The largest `.crisp` upload accepted, in bytes.
    pub max_bundle_bytes: usize,
    /// The CORS origin allowlist. `None` means any origin is allowed (the
    /// lenient default); `Some` restricts cross-origin access to the listed
    /// origins.
    pub allowed_origins: Option<Vec<HeaderValue>>,
}

/// Reads a required environment variable.
fn required(key: &str) -> eyre::Result<String> {
    env::var(key).wrap_err_with(|| format!("required environment variable {key} is not set"))
}

/// Reads an optional environment variable, falling back to `default`.
fn optional(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_owned())
}

impl Config {
    /// Loads configuration from the environment.
    ///
    /// `CRISP_DATABASE_URL` and `CRISP_PLATFORM_SEED` (64 hex chars) are
    /// required; every other setting has a documented default.
    ///
    /// # Errors
    ///
    /// Returns an error if a required variable is missing or malformed.
    pub fn from_env() -> eyre::Result<Self> {
        let database_url = required("CRISP_DATABASE_URL")?;
        let bind_addr = optional("CRISP_BIND_ADDR", "0.0.0.0:8080");

        let seed_hex = required("CRISP_PLATFORM_SEED")?;
        let seed_bytes =
            hex::decode(seed_hex.trim()).wrap_err("CRISP_PLATFORM_SEED is not valid hex")?;
        let platform_seed: [u8; 32] = seed_bytes
            .try_into()
            .map_err(|_| eyre!("CRISP_PLATFORM_SEED must decode to exactly 32 bytes"))?;

        let session_ttl_secs = parse_or(
            &optional("CRISP_SESSION_TTL_SECS", ""),
            DEFAULT_SESSION_TTL_SECS,
        );
        let challenge_ttl_secs = parse_or(
            &optional("CRISP_CHALLENGE_TTL_SECS", ""),
            DEFAULT_CHALLENGE_TTL_SECS,
        );
        let max_bundle_bytes = parse_or(
            &optional("CRISP_MAX_BUNDLE_BYTES", ""),
            DEFAULT_MAX_BUNDLE_BYTES,
        );
        let allowed_origins = parse_allowed_origins(&optional("CRISP_ALLOWED_ORIGINS", ""))?;

        Ok(Self {
            database_url,
            bind_addr,
            platform_seed,
            session_ttl_secs,
            challenge_ttl_secs,
            max_bundle_bytes,
            allowed_origins,
        })
    }
}

/// Parses the `CRISP_ALLOWED_ORIGINS` value into a CORS origin allowlist.
///
/// An empty value yields `None` (any origin allowed). Otherwise the value is a
/// comma-separated list of origins, each parsed as an HTTP header value.
///
/// # Errors
///
/// Returns an error if a listed origin is not a valid header value.
fn parse_allowed_origins(value: &str) -> eyre::Result<Option<Vec<HeaderValue>>> {
    let origins: Vec<HeaderValue> = value
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(|origin| {
            origin.parse::<HeaderValue>().wrap_err_with(|| {
                format!("CRISP_ALLOWED_ORIGINS contains an invalid origin: {origin}")
            })
        })
        .collect::<eyre::Result<_>>()?;
    Ok((!origins.is_empty()).then_some(origins))
}

/// Parses a value, falling back to `default` on an empty or invalid string.
fn parse_or<T: std::str::FromStr>(value: &str, default: T) -> T {
    value.trim().parse().unwrap_or(default)
}
