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
use kynos::middleware::trace::{Level, Trace};
use kynos::openapi::Info;
use kynos::router::group::Group;
use kynos::router::service::Service;
use kynos::server::Server;
use kynos::server::protocol::http1::Http1Config;
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
/// of the panic's payload, rather than dropping the connection and the
/// ingress's pooled keep-alive with it. Recovery needs `panic = "unwind"`,
/// the release default.
///
/// Every request past the CORS interceptor runs inside one `request` span
/// (a preflight CORS answers itself is outside it), opened once [`RequestId`]
/// has assigned the request's id, so each event a handler or its
/// `#[tracing::instrument]` spans emit carries that id. The span closes
/// with the status and latency; the request and response events themselves
/// come from the same [`Trace`] observing the chain.
///
/// Build it with [`Router::build`] and an [`App`] to serve it, or call
/// [`Router::openapi_as`] for its description.
#[must_use]
pub fn router(
    options: &HttpOptions,
) -> Router<App, impl PanicPolicy, impl Sized + use<>, impl Sized + use<>> {
    let request_id = RequestId::new().trust_client(false).source(Random::new());
    let trace = Trace::new().level(Level::Info).correlating(&request_id);

    let api = Group::new(API_PREFIX).mount(routes![
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
        bundles::publish.intercept(BodySize::new(options.max_bundle_bytes)),
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
    ]);

    Router::<App>::new()
        .catch_panics()
        .info(info())
        .security_scheme::<SessionToken>()
        .trusted_proxies(options.proxies.policy())
        .intercept(cors(options))
        .intercept(request_id)
        .intercept(trace.clone().span(&[]))
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
/// `Strict-Transport-Security` is left to the ingress, which terminates TLS
/// and is the only hop that knows the client's scheme.
fn security_headers() -> SecurityHeaders<false, true> {
    SecurityHeaders::new().deny_framing()
}

/// Prepares the listener that serves `service`, holding every connection to
/// `limits`.
///
/// The listener speaks plaintext HTTP/1.1 only: kynos is built without its
/// `http2` and `tls` features, so a prior-knowledge HTTP/2 preface is refused
/// and no certificate is ever loaded. TLS, and HTTP/2 to clients, belong to
/// the ingress in front of it (docs/deployment.md, "Protocol").
///
/// The header timeout bounds both how long a client may take to send a
/// request head and how long an idle keep-alive connection is kept. The body
/// idle timeout answers 408 to an upload that stops sending. Connections past
/// `max_connections` wait in the accept backlog, and with one request in
/// flight per HTTP/1.1 connection that bounds the requests in flight at once.
#[must_use]
pub fn server<C: 'static>(service: Service<C>, limits: &ConnectionLimits) -> Server<C> {
    tracing::info!(
        protocol = "HTTP/1.1",
        header_timeout_secs = limits.header_timeout.as_secs(),
        body_idle_timeout_secs = limits.body_idle_timeout.as_secs(),
        max_connections = limits.max_connections.get(),
        "connection limits"
    );
    Server::new(service)
        .http1(Http1Config::default().header_read_timeout(Some(limits.header_timeout)))
        .request_body_idle_timeout(Some(limits.body_idle_timeout))
        .max_connections(limits.max_connections)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use kynos::Router;
    use kynos::extract::connection::Connection;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    /// The client connection preface every prior-knowledge HTTP/2 (h2c)
    /// connection opens with (RFC 9113 §3.4).
    const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

    /// Writes `request` to a connection the configured listener serves, and
    /// returns everything the server sends within a second, and whether it
    /// closed the connection by then.
    async fn exchange(request: &[u8]) -> (Vec<u8>, bool) {
        let service = Router::<()>::new().build(()).unwrap();
        let server = server(service, &ConnectionLimits::default());
        let (mut client, io) = tokio::io::duplex(64 * 1024);
        let peer: std::net::SocketAddr = "192.0.2.1:50000".parse().unwrap();
        let local: std::net::SocketAddr = "192.0.2.2:8080".parse().unwrap();
        let serving = server
            .serve_io(io, Connection::from_peer(peer, local))
            .unwrap();
        let driver = tokio::spawn(serving);
        client.write_all(request).await.unwrap();
        let mut answer = Vec::new();
        let mut chunk = [0_u8; 4096];
        let closed = loop {
            match tokio::time::timeout(Duration::from_secs(1), client.read(&mut chunk)).await {
                Ok(Ok(0)) => break true,
                Ok(Ok(read)) => answer.extend_from_slice(&chunk[..read]),
                Ok(Err(_)) => break true,
                Err(_) => break false,
            }
        };
        driver.abort();
        (answer, closed)
    }

    #[tokio::test]
    async fn http1_1_is_served() {
        // The empty router answers every operation 501; the point is that
        // an HTTP/1.1 response comes back at all.
        let (answer, closed) =
            exchange(b"GET /nowhere HTTP/1.1\r\nHost: crisp\r\nConnection: close\r\n\r\n").await;
        assert!(
            answer.starts_with(b"HTTP/1.1 501"),
            "{}",
            String::from_utf8_lossy(&answer)
        );
        assert!(closed, "Connection: close is honoured");
    }

    #[tokio::test]
    async fn a_prior_knowledge_http2_connection_is_not_served() {
        // An HTTP/2 server answers the preface with a SETTINGS frame (a 9-byte
        // frame header whose type octet, the fourth, is 0x4) and holds the
        // connection open; an HTTP/1.1-only one refuses it and closes.
        let (answer, closed) = exchange(H2_PREFACE).await;
        assert!(
            answer.get(3) != Some(&0x4),
            "the listener answered HTTP/2: {answer:?}"
        );
        assert!(
            answer.is_empty() || answer.starts_with(b"HTTP/1.1 "),
            "{}",
            String::from_utf8_lossy(&answer)
        );
        assert!(closed, "the listener held an HTTP/2 connection open");
    }
}
