//! The HTTP router: every route, wired to its handler with the shared layers.

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::{Json, Router};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;
use crate::{auth, bundles, search, version};

/// `GET /api/v1/health` — a trivial liveness probe.
async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Builds the application router from the shared [`AppState`].
///
/// Bundle uploads can be large, so the default body limit is raised to the
/// configured ceiling. Every response is stamped with version headers.
pub fn router(state: AppState) -> Router {
    let max_body = state.config.max_bundle_bytes;
    let cors = if state.config.cors_permissive {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
    };

    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/server-info", get(version::server_info))
        .route("/api/v1/platform-keys", get(version::platform_keys))
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/challenge", post(auth::challenge))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/logout", post(auth::logout))
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
