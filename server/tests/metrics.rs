//! The operational metrics: the API router records every response into
//! them, and the metrics listener serves them on an address of its own -
//! never on the public API.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{MockStorage, platform, test_config};
use crisp_server::metrics::exporter::{
    Exporter, METRICS_PATH, MetricsListener, Pool, PoolProbe, serve_failure, service,
};
use crisp_server::metrics::{Metrics, PoolUsage};
use crisp_server::state::App;
use crisp_server::storage::Storage;
use kynos::http::StatusCode;
use kynos::server::error::ServerError;
use kynos::test::TestClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// How long any one exchange may take before the test fails rather than hangs.
const BOUND: Duration = Duration::from_secs(10);

/// A pool that always reports the same occupancy.
struct FixedPool(PoolUsage);

impl PoolProbe for FixedPool {
    fn usage(&self) -> PoolUsage {
        self.0
    }
}

/// The real API router, observed by a fresh registry; returns both.
fn api() -> (TestClient<App>, Metrics) {
    let config = test_config();
    let storage: Arc<dyn Storage> = Arc::new(MockStorage::default());
    let app = App::new(storage, Arc::new(platform()), Arc::new(config.clone()));
    let metrics = app.metrics.clone();
    let service = crisp_server::router(&config.http)
        .observe(metrics.observer())
        .build(app)
        .expect("router builds");
    (TestClient::new(service), metrics)
}

/// An in-process client for the metrics service over `metrics` and `pool`.
fn exporter(metrics: &Metrics, pool: Option<PoolUsage>) -> TestClient<Exporter> {
    let pool = Pool(pool.map(|usage| Arc::new(FixedPool(usage)) as Arc<dyn PoolProbe>));
    let service = service(Exporter {
        metrics: metrics.clone(),
        pool,
    })
    .expect("metrics service builds");
    TestClient::new(service)
}

#[tokio::test]
async fn the_api_records_each_response_by_operation_and_status() {
    let (client, metrics) = api();
    client
        .get("/api/v1/health")
        .send()
        .await
        .assert_status(StatusCode::OK);
    client
        .get("/api/v1/health")
        .send()
        .await
        .assert_status(StatusCode::OK);
    client
        .get("/api/v1/bundles/doc_missing")
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
    client
        .get("/api/v1/no-such-route")
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);

    let text = metrics.encode().unwrap();
    assert!(
        text.contains("crisp_http_requests_total{operation=\"health\",status=\"200\"} 2\n"),
        "{text}"
    );
    assert!(
        text.contains("crisp_http_request_duration_seconds_count{operation=\"health\"} 2\n"),
        "{text}"
    );
    assert!(
        text.contains("crisp_http_requests_total{operation=\"get_latest\",status=\"404\"} 1\n"),
        "{text}"
    );
    assert!(
        text.contains("crisp_http_requests_total{operation=\"<unmatched>\",status=\"404\"} 1\n"),
        "{text}"
    );
}

#[tokio::test]
async fn the_public_api_serves_no_metrics() {
    let (client, _) = api();
    for path in ["/metrics", "/api/v1/metrics"] {
        client
            .get(path)
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn a_scrape_samples_the_pool_and_serves_openmetrics_text() {
    let metrics = Metrics::new();
    metrics.record_request("search", 200, Duration::from_millis(2));
    let client = exporter(
        &metrics,
        Some(PoolUsage {
            open: 7,
            idle: 2,
            max: 10,
        }),
    );

    let response = client.get(METRICS_PATH).send().await;
    response.assert_status(StatusCode::OK);
    let content_type = response.header("content-type").unwrap_or_default();
    assert!(
        content_type.starts_with("application/openmetrics-text"),
        "{content_type}"
    );
    let text = response.text();
    assert!(text.contains("crisp_db_pool_connections 7\n"), "{text}");
    assert!(
        text.contains("crisp_db_pool_idle_connections 2\n"),
        "{text}"
    );
    assert!(
        text.contains("crisp_db_pool_max_connections 10\n"),
        "{text}"
    );
    assert!(
        text.contains("crisp_http_requests_total{operation=\"search\",status=\"200\"} 1\n"),
        "{text}"
    );
    assert!(text.ends_with("# EOF\n"), "{text}");
}

#[tokio::test]
async fn the_metrics_listener_serves_only_metrics() {
    let client = exporter(&Metrics::new(), None);
    client
        .get("/api/v1/health")
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

/// Sends `request` on a fresh connection and reads until the server closes it.
async fn exchange(address: std::net::SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(address).await.expect("connects");
    stream.write_all(request.as_bytes()).await.expect("sends");
    let mut received = Vec::new();
    stream.read_to_end(&mut received).await.expect("reads");
    String::from_utf8_lossy(&received).into_owned()
}

#[tokio::test]
async fn the_listener_serves_a_scrape_over_a_socket_and_stops_on_shutdown() {
    let metrics = Metrics::new();
    metrics.record_request("health", 200, Duration::ZERO);
    let exporter = Exporter {
        metrics,
        pool: Pool(None),
    };
    let listener = MetricsListener::start("127.0.0.1:0", exporter)
        .await
        .expect("the metrics listener binds");
    let address = listener.local_addr();

    let response = timeout(
        BOUND,
        exchange(
            address,
            "GET /metrics HTTP/1.1\r\nhost: metrics\r\nconnection: close\r\n\r\n",
        ),
    )
    .await
    .expect("the scrape answers");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(
        response.contains("crisp_http_requests_total{operation=\"health\",status=\"200\"} 1"),
        "{response}"
    );

    timeout(BOUND, listener.shutdown())
        .await
        .expect("the listener stops promptly");
    // A blocking connect never yields to the runtime, so it sees only what
    // `shutdown` itself finished, not a stop the serving task makes later.
    assert!(
        std::net::TcpStream::connect(address).is_err(),
        "nothing listens after shutdown"
    );
}

#[test]
fn a_requested_shutdown_is_not_a_serve_failure() {
    assert!(serve_failure(Ok(())).is_none());
    let forced = kynos::Error::Server(ServerError::ShutdownForced);
    assert!(serve_failure(Err(forced)).is_none());
    let timed_out = kynos::Error::Server(ServerError::ShutdownTimeout {
        timeout: Duration::ZERO,
    });
    assert!(serve_failure(Err(timed_out)).is_none());
    let failed = kynos::Error::Server(ServerError::NoListeners);
    assert!(matches!(
        serve_failure(Err(failed)),
        Some(kynos::Error::Server(ServerError::NoListeners))
    ));
}

#[tokio::test]
async fn a_taken_address_fails_the_start() {
    let taken = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = taken.local_addr().unwrap().to_string();
    let exporter = Exporter {
        metrics: Metrics::new(),
        pool: Pool(None),
    };
    assert!(MetricsListener::start(&address, exporter).await.is_err());
}
