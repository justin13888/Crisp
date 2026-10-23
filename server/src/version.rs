//! Liveness, API/format version negotiation, and platform-key publication
//! (plan Stage 15).
//!
//! Clients and servers can drift: a client may ship an older or newer build
//! of `crisp-core` than the server. To make drift visible and recoverable:
//!
//! - the URL carries the API version (`/api/v1`);
//! - every response carries `crisp-api-version` and `crisp-format-version`
//!   headers (added by the [`VersionStamp`] interceptor, and therefore
//!   declared on every operation in the OpenAPI document);
//! - `GET /api/v1/server-info` advertises the exact supported ranges and the
//!   platform keys, so a client can detect a mismatch before it publishes.

use std::convert::Infallible;

use kynos::http;
use kynos::middleware::{Continued, Interceptor, Next};
use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::platform::PlatformKey;
use crate::state::Platform;

/// The single API version this build serves.
pub const API_VERSION: &str = "v1";

// The format version is advertised as a static header; keep that string and
// `crisp_core`'s constant from drifting apart.
const _: () = assert!(crisp_core::FORMAT_VERSION == 0x01_0000);
/// The `crisp-format-version` header value (decimal of
/// `crisp_core::FORMAT_VERSION`).
pub const FORMAT_VERSION_HEADER: &str = "65536";

/// The version of the `crisp-core` crate this server links.
///
/// `crisp-core` exports no native version constant (its `core_version()` is
/// WASM-only), so this reads the workspace package version, which `core` and
/// `server` share through `version.workspace = true` and therefore always
/// equals core's.
#[must_use]
pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The version headers stamped onto every response.
#[derive(Debug, Clone, HeaderParams)]
pub struct VersionHeaders {
    /// The API version that served the response.
    #[header(rename = "crisp-api-version")]
    pub api_version: String,
    /// The bundle format version this server implements.
    #[header(rename = "crisp-format-version")]
    pub format_version: String,
}

/// Interceptor stamping [`VersionHeaders`] onto every response.
#[derive(Debug, Clone, Copy, Default)]
pub struct VersionStamp;

impl<C: Sync + 'static> Interceptor<C> for VersionStamp {
    type Reads = ();
    type Adds = VersionHeaders;
    type Short = Infallible;

    async fn intercept(
        &self,
        request: http::Request,
        (): (),
        _context: &C,
        next: Next<'_, C>,
    ) -> Result<Continued<VersionHeaders>, Infallible> {
        Ok(next.run(request).await.with_headers(VersionHeaders {
            api_version: API_VERSION.to_owned(),
            format_version: FORMAT_VERSION_HEADER.to_owned(),
        }))
    }
}

/// The liveness state reported by `GET /health`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// The process is serving requests.
    Ok,
}

/// The body of `GET /api/v1/health`.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct Health {
    /// Always `ok` when the server answers at all.
    pub status: HealthStatus,
}

/// A supported-version window.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct FormatVersionRange {
    /// The format version this build implements.
    pub current: u64,
    /// The lowest bundle `min_v` this build can open.
    pub min_supported: u64,
    /// The highest bundle format version this build accepts.
    pub max_supported: u64,
}

/// A published platform key, with its validity window.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
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
#[must_use]
pub fn platform_key_info(platform: &PlatformKey) -> Vec<PlatformKeyInfo> {
    let keyset = platform.public_keyset();
    vec![PlatformKeyInfo {
        usr_id: platform.user_id().to_owned(),
        ed25519: hex::encode(&keyset.ed25519),
        ml_dsa_65: hex::encode(&keyset.ml_dsa_65),
        valid_from: None,
        valid_until: None,
    }]
}

/// A trivial liveness probe.
#[kynos::get("/health")]
pub async fn health() -> Json<Health> {
    Json(Health {
        status: HealthStatus::Ok,
    })
}

/// Advertises supported API and format versions and the platform keys.
#[kynos::get("/server-info")]
pub async fn server_info(Inject(platform): Inject<Platform>) -> Json<ServerInfo> {
    Json(ServerInfo {
        api_versions: vec![API_VERSION.to_owned()],
        format_version: FormatVersionRange {
            current: crisp_core::FORMAT_VERSION,
            min_supported: crisp_core::MIN_READER_VERSION,
            max_supported: crisp_core::FORMAT_VERSION,
        },
        platform_keys: platform_key_info(&platform),
        core_version: core_version().to_owned(),
    })
}

/// Lists just the platform keys, for offline endorsement verification.
#[kynos::get("/platform-keys")]
pub async fn platform_keys(Inject(platform): Inject<Platform>) -> Json<Vec<PlatformKeyInfo>> {
    Json(platform_key_info(&platform))
}
