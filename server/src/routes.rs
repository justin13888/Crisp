//! The HTTP router: every operation under `/api/v1`, plus the router-wide
//! interceptors (CORS, request id, version headers, security headers) and the
//! request tracer; and the listener that serves it.
//!
//! [`router`] is the single definition of the API surface. `main` builds and
//! serves it through [`server`]; the tests drive it in-process;
//! `tests/openapi.rs` emits its OpenAPI 3.2 description into
//! `spec/openapi.json`.

use std::time::Duration;

use kynos::middleware::catch_panic::PanicPolicy;
use kynos::middleware::cors::Cors;
use kynos::middleware::limits::body_size::BodySize;
use kynos::middleware::request_id::{Random, RequestId};
use kynos::middleware::security_headers::SecurityHeaders;
use kynos::middleware::trace::Trace;
use kynos::openapi::Info;
use kynos::router::group::Group;
use kynos::router::service::Service;
use kynos::server::Server;
use kynos::server::protocol::http1::Http1Config;
use kynos::server::protocol::http2::Http2Config;
use kynos::{Router, routes};

use crate::auth::SessionToken;
use crate::config::{ConnectionLimits, HttpOptions};
use crate::state::App;
use crate::version::VersionStamp;
use crate::{auth, bundles, moderation, quota, revocations, search, version};

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
///
/// A handler that panics is answered with an RFC 9457 500 that carries none
/// of the panic's payload, rather than dropping the connection (and, over
/// HTTP/2, every other request in flight on it). Recovery needs
/// `panic = "unwind"`, the release default.
///
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
            version::ready,
            version::server_info,
            version::platform_keys,
            auth::register,
            auth::challenge,
            auth::login,
            auth::sessions::logout,
            auth::sessions::logout_all,
            auth::sessions::sessions,
            auth::account_profile,
            quota::account_usage,
            bundles::get_latest,
            bundles::get_exact,
            search::browse,
            search::search,
        ])
        // A second list: a single `routes!` mounts at most 16 operations.
        .mount(routes![
            auth::deletion::delete_account,
            bundles::unpublish::unpublish_document,
            bundles::unpublish::unpublish_revision,
            moderation::reports::file_report,
            moderation::reports::list_reports,
            moderation::reports::resolve_report,
            moderation::takedowns::take_down,
            moderation::takedowns::lift_takedown,
            moderation::accounts::suspend,
            moderation::accounts::unsuspend,
            revocations::revocations,
        ])
        .mount(
            routes![bundles::publish]
                .0
                .intercept(BodySize::new(options.max_bundle_bytes)),
        );

    Router::<App>::new()
        .catch_panics()
        .info(info())
        .security_scheme::<SessionToken>()
        .trusted_proxies(options.proxies.policy())
        .intercept(cors(options))
        .intercept(request_id)
        .intercept(VersionStamp)
        .intercept(security_headers())
        .observe(trace)
        .group(api)
}

/// The security headers every response carries: `Cache-Control: no-store`,
/// `Referrer-Policy: no-referrer`, `X-Content-Type-Options: nosniff`, and
/// `X-Frame-Options: DENY`.
///
/// `nosniff` keeps a browser from rendering a downloaded bundle, which is
/// served as `application/octet-stream`, as anything else. Every response is
/// per-caller API data, so none is stored by a shared cache.
/// `Strict-Transport-Security` is left to the TLS-terminating reverse proxy,
/// the only hop that knows the client's scheme in every deployment.
fn security_headers() -> SecurityHeaders<false, true> {
    SecurityHeaders::new().deny_framing()
}

/// Prepares the listener that serves `service`, holding every connection to
/// `limits`.
///
/// The header timeout bounds both how long a client may take to send a
/// request head and how long an idle keep-alive connection (HTTP/1, or
/// HTTP/2 with no stream in flight) is kept. The body idle timeout answers 408
/// to an upload that stops sending. Connections past `max_connections` wait
/// in the accept backlog; with HTTP/2's per-connection stream cap that bounds
/// the requests in flight at once.
#[must_use]
pub fn server(service: Service<App>, limits: &ConnectionLimits) -> Server<App> {
    tracing::info!(
        header_timeout_secs = limits.header_timeout.as_secs(),
        body_idle_timeout_secs = limits.body_idle_timeout.as_secs(),
        max_connections = limits.max_connections.get(),
        max_streams_per_connection = limits.max_streams_per_connection,
        "connection limits"
    );
    Server::new(service)
        .http1(Http1Config::default().header_read_timeout(Some(limits.header_timeout)))
        .http2(
            Http2Config::default()
                .idle_timeout(Some(limits.header_timeout))
                .max_concurrent_streams(limits.max_streams_per_connection),
        )
        .request_body_idle_timeout(Some(limits.body_idle_timeout))
        .max_connections(limits.max_connections)
}
