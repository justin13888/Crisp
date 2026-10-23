//! The HTTP router: every operation under `/api/v1`, plus the router-wide
//! interceptors (CORS, request id, version headers) and the request tracer.
//!
//! [`router`] is the single definition of the API surface. `main` builds and
//! serves it; the tests drive it in-process; `tests/openapi.rs` emits its
//! OpenAPI 3.2 description into `spec/openapi.json`.

use std::time::Duration;

use kynos::middleware::catch_panic::PanicPolicy;
use kynos::middleware::cors::Cors;
use kynos::middleware::limits::body_size::BodySize;
use kynos::middleware::request_id::{Random, RequestId};
use kynos::middleware::trace::Trace;
use kynos::openapi::Info;
use kynos::router::group::Group;
use kynos::{Router, routes};

use crate::auth::SessionToken;
use crate::config::HttpOptions;
use crate::state::App;
use crate::version::VersionStamp;
use crate::{auth, bundles, search, version};

/// The prefix every operation is mounted under.
pub const API_PREFIX: &str = "/api/v1";

/// How long a browser may cache a CORS preflight answer.
const PREFLIGHT_MAX_AGE: Duration = Duration::from_secs(3600);

/// Builds the server's CORS policy.
///
/// Crisp authenticates with a bearer token in the `Authorization` header, not
/// a cookie, so cross-origin requests carry no ambient credentials and a
/// wildcard origin is safe — a hostile page cannot obtain a victim's token.
/// The default is therefore lenient, so the client works as a web app, an
/// installed PWA, or a desktop webview without per-origin configuration; set
/// `CRISP_ALLOWED_ORIGINS` to restrict it. Credentials are never allowed.
fn cors(options: &HttpOptions) -> Cors {
    let cors = Cors::new()
        .allow_headers(["authorization", "content-type"])
        .expose_headers(["crisp-api-version", "crisp-format-version", "x-request-id"])
        .max_age(PREFLIGHT_MAX_AGE);
    match &options.allowed_origins {
        None => cors.allow_any_origin(),
        Some(origins) => cors.allow_origins(origins.clone()),
    }
}

/// The API's `info` block.
fn info() -> Info {
    Info {
        description: Some(
            "The Crisp backend: stores, verifies, endorses, and indexes recipe bundles. \
             Errors are RFC 9457 `application/problem+json` documents carrying a stable \
             `code` member."
                .to_owned(),
        ),
        ..Info::new("Crisp API", env!("CARGO_PKG_VERSION"))
    }
}

/// Builds the application router.
///
/// Uploads are capped per operation: `POST /bundles` accepts up to
/// `options.max_bundle_bytes`; every other body uses kynos's 2 MiB default.
/// Build it with [`Router::build`] and an [`App`] to serve it, or call
/// [`Router::openapi_as`] for its description.
#[must_use]
pub fn router(
    options: &HttpOptions,
) -> Router<App, impl PanicPolicy, impl Sized + use<>, impl Sized + use<>> {
    let request_id = RequestId::new().trust_client(false).source(Random::new());
    let trace = Trace::new()
        .level(tracing::Level::INFO)
        .correlating(&request_id);

    let api = Group::new(API_PREFIX)
        .mount(routes![
            version::health,
            version::server_info,
            version::platform_keys,
            auth::register,
            auth::challenge,
            auth::login,
            auth::logout,
            auth::account_profile,
            bundles::get_latest,
            bundles::get_exact,
            search::browse,
            search::search,
        ])
        .mount(
            routes![bundles::publish]
                .0
                .intercept(BodySize::new(options.max_bundle_bytes)),
        );

    Router::<App>::new()
        .info(info())
        .security_scheme::<SessionToken>()
        .intercept(cors(options))
        .intercept(request_id)
        .intercept(VersionStamp)
        .observe(trace)
        .group(api)
}
