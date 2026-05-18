//! API and format version negotiation (plan Stage 15).
//!
//! Clients and servers can drift: a client may ship an older or newer build
//! of `crisp-core` than the server. To make drift visible and recoverable:
//!
//! - the URL carries the API version (`/api/v1`);
//! - every response carries `Crisp-Api-Version` and `Crisp-Format-Version`
//!   headers (added by [`version_headers`]);
//! - `GET /api/v1/server-info` advertises the exact supported ranges and the
//!   platform keys, so a client can detect a mismatch before it publishes.

use axum::Json;
use axum::extract::{Request, State};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use serde::Serialize;

use crate::state::AppState;

/// The single API version this build serves.
pub const API_VERSION: &str = "v1";

// The format version is advertised as a static header; keep that string and
// `crisp_core`'s constant from drifting apart.
const _: () = assert!(crisp_core::FORMAT_VERSION == 0x01_0000);
/// The `Crisp-Format-Version` header value (decimal of `crisp_core::FORMAT_VERSION`).
const FORMAT_VERSION_HEADER: &str = "65536";

/// Middleware that stamps the API and format version onto every response.
pub async fn version_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("crisp-api-version", HeaderValue::from_static(API_VERSION));
    headers.insert(
        "crisp-format-version",
        HeaderValue::from_static(FORMAT_VERSION_HEADER),
    );
    response
}

/// A supported-version window.
#[derive(Debug, Serialize)]
pub struct FormatVersionRange {
    /// The format version this build implements.
    pub current: u64,
    /// The lowest bundle `min_v` this build can open.
    pub min_supported: u64,
    /// The highest bundle format version this build accepts.
    pub max_supported: u64,
}

/// A published platform key, with its validity window.
#[derive(Debug, Serialize)]
pub struct PlatformKeyInfo {
    /// The platform's `usr…` identifier.
    pub usr_id: String,
    /// The Ed25519 public key, hex-encoded.
    pub ed25519: String,
    /// The ML-DSA-65 public key, hex-encoded.
    pub ml_dsa_65: String,
    /// The start of the key's validity window, or `null` if open-ended.
    pub valid_from: Option<i64>,
    /// The end of the key's validity window, or `null` if open-ended.
    pub valid_until: Option<i64>,
}

/// The body of `GET /api/v1/server-info`.
#[derive(Debug, Serialize)]
pub struct ServerInfo {
    /// Every API version this server serves.
    pub api_versions: Vec<String>,
    /// The format-version window this server supports.
    pub format_version: FormatVersionRange,
    /// The platform keys clients use to verify endorsements offline.
    pub platform_keys: Vec<PlatformKeyInfo>,
    /// The `crisp-core` build version this server links.
    pub core_version: String,
}

/// Builds the [`PlatformKeyInfo`] list for the running server.
fn platform_key_info(state: &AppState) -> Vec<PlatformKeyInfo> {
    let keyset = state.platform.public_keyset();
    let usr_id = state.platform.user_id().unwrap_or_default();
    vec![PlatformKeyInfo {
        usr_id,
        ed25519: hex::encode(&keyset.ed25519),
        ml_dsa_65: hex::encode(&keyset.ml_dsa_65),
        valid_from: None,
        valid_until: None,
    }]
}

/// `GET /api/v1/server-info` — advertises supported versions and keys.
pub async fn server_info(State(state): State<AppState>) -> Json<ServerInfo> {
    Json(ServerInfo {
        api_versions: vec![API_VERSION.to_owned()],
        format_version: FormatVersionRange {
            current: crisp_core::FORMAT_VERSION,
            min_supported: crisp_core::MIN_READER_VERSION,
            max_supported: crisp_core::FORMAT_VERSION,
        },
        platform_keys: platform_key_info(&state),
        core_version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

/// `GET /api/v1/platform-keys` — just the platform keys, for offline
/// endorsement verification.
pub async fn platform_keys(State(state): State<AppState>) -> Json<Vec<PlatformKeyInfo>> {
    Json(platform_key_info(&state))
}
