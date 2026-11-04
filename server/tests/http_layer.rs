//! The HTTP layer over a real socket: the listener's timeouts and connection
//! cap, the trusted-proxy model the per-address rate limits key by, and the
//! security headers every response carries.
//!
//! Each test serves the real router over mock storage on an ephemeral
//! loopback port and speaks raw HTTP/1.1 to it, so a slow or stalled client
//! can be played exactly.

mod common;

use std::net::SocketAddr;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{MockStorage, platform, test_config};
use crisp_server::config::{Config, ConnectionLimits, ProxyTrust};
use crisp_server::limits::Budget;
use crisp_server::state::App;
use crisp_server::storage::Storage;
use kynos::http::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// How long any one exchange may take before the test fails rather than hangs.
const BOUND: Duration = Duration::from_secs(10);

/// Serves the real router under `config` on an ephemeral loopback port.
async fn serve(config: Config) -> SocketAddr {
    let storage: Arc<dyn Storage> = Arc::new(MockStorage::default());
    let limits = config.http.connections;
    let service = crisp_server::router(&config.http)
        .build(App::new(storage, Arc::new(platform()), Arc::new(config)))
        .expect("router builds");
    let bound = crisp_server::routes::server(service, &limits)
        .bind("127.0.0.1:0")
        .prepare()
        .await
        .expect("the listener binds");
    let address = bound.local_addrs()[0];
    tokio::spawn(bound.serve());
    address
}

/// A configuration whose listener is held to `limits`.
fn limited(limits: ConnectionLimits) -> Config {
    let mut config = test_config();
    config.http.connections = limits;
    config
}

/// Sends `request` on a fresh connection and reads until the server closes it.
async fn exchange(address: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(address).await.expect("connects");
    stream.write_all(request.as_bytes()).await.expect("sends");
    read_to_close(&mut stream).await
}

/// Reads everything the server sends until it closes the connection.
async fn read_to_close(stream: &mut TcpStream) -> String {
    let mut response = Vec::new();
    timeout(BOUND, stream.read_to_end(&mut response))
        .await
        .expect("the server answers within the bound")
        .expect("reads");
    String::from_utf8_lossy(&response).into_owned()
}

/// The status code on a raw response's status line.
fn status(response: &str) -> u16 {
    response
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no status line in {response:?}"))
}

/// A `GET` that closes its connection, sent as from `forwarded_for` when given.
fn get(path: &str, forwarded_for: Option<&str>) -> String {
    let forwarded = forwarded_for.map_or_else(String::new, |client| {
        format!("X-Forwarded-For: {client}\r\n")
    });
    format!("GET {path} HTTP/1.1\r\nHost: crisp.test\r\n{forwarded}Connection: close\r\n\r\n")
}

#[tokio::test]
async fn a_request_head_that_never_finishes_is_closed_at_the_header_timeout() {
    let address = serve(limited(ConnectionLimits {
        header_timeout: Duration::from_secs(1),
        ..ConnectionLimits::default()
    }))
    .await;
    let started = Instant::now();
    let response = exchange(
        address,
        "GET /api/v1/health HTTP/1.1\r\nHost: crisp.test\r\n",
    )
    .await;
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(900),
        "closed after {elapsed:?}, before the timeout"
    );
    assert!(
        !response.contains(" 200 "),
        "an unfinished head was served: {response:?}"
    );
}

#[tokio::test]
async fn an_upload_that_stalls_is_answered_408_at_the_body_idle_timeout() {
    let address = serve(limited(ConnectionLimits {
        body_idle_timeout: Duration::from_secs(1),
        ..ConnectionLimits::default()
    }))
    .await;
    // Promises 100 bytes, sends 10, then goes quiet.
    let request = "POST /api/v1/auth/challenge HTTP/1.1\r\nHost: crisp.test\r\n\
                   Content-Type: application/json\r\nContent-Length: 100\r\n\r\n{\"usernam";
    let started = Instant::now();
    let response = exchange(address, request).await;
    assert_eq!(status(&response), StatusCode::REQUEST_TIMEOUT.as_u16());
    assert!(started.elapsed() >= Duration::from_millis(900));
}

#[tokio::test]
async fn connections_past_the_cap_wait_until_one_closes() {
    let address = serve(limited(ConnectionLimits {
        max_connections: NonZeroUsize::MIN,
        ..ConnectionLimits::default()
    }))
    .await;

    // The first connection is served and kept alive, holding the one permit.
    let mut first = TcpStream::connect(address).await.expect("connects");
    let keep_alive = "GET /api/v1/health HTTP/1.1\r\nHost: crisp.test\r\n\r\n";
    first.write_all(keep_alive.as_bytes()).await.expect("sends");
    let mut buffer = [0u8; 1024];
    let read = timeout(BOUND, first.read(&mut buffer))
        .await
        .expect("the first connection is served")
        .expect("reads");
    assert_eq!(status(&String::from_utf8_lossy(&buffer[..read])), 200);

    // A second connection is not served while the first is open...
    let mut second = TcpStream::connect(address).await.expect("connects");
    let request = get("/api/v1/health", None);
    second.write_all(request.as_bytes()).await.expect("sends");
    let waiting = timeout(Duration::from_millis(500), second.read(&mut buffer)).await;
    assert!(waiting.is_err(), "a connection past the cap was served");

    // ...and is once the first closes.
    drop(first);
    assert_eq!(status(&read_to_close(&mut second).await), 200);
}

/// A configuration allowing one browse or search per client per hour.
fn one_query(proxies: ProxyTrust) -> Config {
    let mut config = test_config();
    config.http.limits.query = Budget::new(NonZeroU32::MIN, Duration::from_secs(3600));
    config.http.proxies = proxies;
    config
}

#[tokio::test]
async fn a_trusted_proxy_names_the_client_rate_limits_count() {
    let trusted = ProxyTrust {
        networks: vec!["127.0.0.1/32".parse().expect("a network")],
        ..ProxyTrust::default()
    };
    let address = serve(one_query(trusted)).await;
    let from = |client| get("/api/v1/recipes", Some(client));

    assert_eq!(status(&exchange(address, &from("203.0.113.1")).await), 200);
    assert_eq!(status(&exchange(address, &from("203.0.113.1")).await), 429);
    // Another client behind the same proxy has its own budget.
    assert_eq!(status(&exchange(address, &from("203.0.113.2")).await), 200);
    // Only the hop the proxy wrote counts: a client-supplied element before
    // it does not buy a fresh budget.
    let spoofed = from("198.51.100.9, 203.0.113.2");
    assert_eq!(status(&exchange(address, &spoofed).await), 429);
}

#[tokio::test]
async fn an_untrusted_peer_cannot_choose_its_address() {
    let address = serve(one_query(ProxyTrust::default())).await;
    let from = |client| get("/api/v1/recipes", Some(client));

    assert_eq!(status(&exchange(address, &from("203.0.113.1")).await), 200);
    // Nobody is trusted, so the header is ignored and the peer is the client.
    assert_eq!(status(&exchange(address, &from("203.0.113.2")).await), 429);
}

#[tokio::test]
async fn every_response_carries_the_security_headers() {
    let address = serve(test_config()).await;
    for path in ["/api/v1/health", "/api/v1/bundles/not-a-doc-id"] {
        let response = exchange(address, &get(path, None))
            .await
            .to_ascii_lowercase();
        for header in [
            "x-content-type-options: nosniff",
            "cache-control: no-store",
            "referrer-policy: no-referrer",
            "x-frame-options: deny",
        ] {
            assert!(
                response.contains(header),
                "{path} lacks {header}: {response}"
            );
        }
        assert!(
            !response.contains("strict-transport-security"),
            "HSTS is the TLS proxy's to send: {response}"
        );
    }
}
