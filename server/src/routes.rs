//! The HTTP router: every route, wired to its handler with the shared layers.

use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::{Json, Router};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::state::AppState;
use crate::{auth, bundles, search, version};

/// `GET /api/v1/health` — a trivial liveness probe.
async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Builds the server's CORS policy.
///
/// Crisp authenticates with a bearer token in the `Authorization` header, not
/// a cookie, so cross-origin requests carry no ambient credentials and a
/// wildcard origin is safe — a hostile page cannot obtain a victim's token.
/// The policy is deliberately lenient so the client works as a web app, an
/// installed PWA, or a desktop (Tauri/Electron) webview without per-origin
/// configuration. Set `CRISP_ALLOWED_ORIGINS` to restrict it.
///
/// Credentials are intentionally *not* allowed: bearer-token auth needs none,
/// and `Any` origin/headers/methods are incompatible with credentialed CORS.
fn cors_layer(config: &Config) -> CorsLayer {
    // `Any` covers the `Authorization` header and the non-safelisted content
    // types (`application/json`, `application/crisp+cbor`) that trigger a
    // preflight; exposing all headers lets browser JS read the
    // `crisp-api-version` / `crisp-format-version` response headers.
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any)
        .max_age(Duration::from_secs(3600));
    match &config.allowed_origins {
        None => cors.allow_origin(Any),
        Some(origins) => cors.allow_origin(AllowOrigin::list(origins.iter().cloned())),
    }
}

/// Builds the application router from the shared [`AppState`].
///
/// Bundle uploads can be large, so the default body limit is raised to the
/// configured ceiling. Every response is stamped with version headers.
pub fn router(state: AppState) -> Router {
    let max_body = state.config.max_bundle_bytes;
    let cors = cors_layer(&state.config);

    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/server-info", get(version::server_info))
        .route("/api/v1/platform-keys", get(version::platform_keys))
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/challenge", post(auth::challenge))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/accounts/{usr_id}", get(auth::account_profile))
        .route("/api/v1/bundles", post(bundles::publish))
        .route("/api/v1/bundles/{doc_id}", get(bundles::get_latest))
        .route(
            "/api/v1/bundles/{doc_id}/{revision}",
            get(bundles::get_exact),
        )
        .route("/api/v1/recipes", get(search::browse))
        .route("/api/v1/search", get(search::search))
        .layer(DefaultBodyLimit::max(max_body))
        .layer(axum::middleware::from_fn(version::version_headers))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
