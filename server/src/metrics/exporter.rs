//! The metrics listener: `GET /metrics` on an address of its own.
//!
//! It is a separate kynos service with one operation, bound to
//! `CRISP_METRICS_BIND_ADDR` (loopback by default), so the public listener
//! and the reverse proxy in front of it never serve metrics, and the
//! operation is not part of the public OpenAPI document. Each scrape samples
//! the database pool, then encodes the registry.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use kynos::Provider;
use kynos::extract::body::binary::Binary;
use kynos::http::media::MediaType;
use kynos::prelude::*;
use kynos::router::service::Service;
use kynos::server::Server;
use kynos::server::shutdown::Shutdown;
use kynos::{Router, routes};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use super::{Metrics, PoolUsage};
use crate::error::Internal;

/// How long in-flight scrapes may drain after the listener is told to stop.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// The path the exposition is served at.
pub const METRICS_PATH: &str = "/metrics";

/// Reports the database pool's occupancy when a scrape asks.
pub trait PoolProbe: Send + Sync {
    /// The pool's occupancy now.
    fn usage(&self) -> PoolUsage;
}

impl PoolProbe for sea_orm::sqlx::PgPool {
    fn usage(&self) -> PoolUsage {
        PoolUsage {
            open: self.size(),
            idle: u32::try_from(self.num_idle()).unwrap_or(u32::MAX),
            max: self.options().get_max_connections(),
        }
    }
}

/// The pool a scrape samples; `None` leaves the pool gauges untouched.
#[derive(Clone)]
pub struct Pool(pub Option<Arc<dyn PoolProbe>>);

/// The context the metrics listener serves with.
#[derive(Provider)]
pub struct Exporter {
    /// The registry being exposed.
    pub metrics: Metrics,
    /// The pool sampled at each scrape.
    pub pool: Pool,
}

/// `application/openmetrics-text`, the exposition format Prometheus
/// negotiates first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenMetricsText;

impl MediaType for OpenMetricsText {
    const MEDIA_TYPE: &'static str = "application/openmetrics-text; version=1.0.0; charset=utf-8";
}

/// Samples the pool and returns the registry as OpenMetrics text.
#[kynos::get("/metrics")]
#[tracing::instrument(level = "trace", skip_all)]
pub async fn scrape(
    Inject(metrics): Inject<Metrics>,
    Inject(pool): Inject<Pool>,
) -> Result<Binary<OpenMetricsText>, Internal> {
    if let Some(pool) = &pool.0 {
        metrics.record_pool(pool.usage());
    }
    let text = metrics
        .encode()
        .map_err(|e| Internal::logged("encoding metrics", &e))?;
    Ok(Binary::new(text.into_bytes()))
}

/// Builds the metrics service over `exporter`.
///
/// # Errors
///
/// Returns an error if kynos rejects the router.
pub fn service(exporter: Exporter) -> Result<Service<Exporter>, kynos::Error> {
    Router::<Exporter>::new()
        .mount(routes![scrape])
        .build(exporter)
}

/// A running metrics listener and the handle that stops it.
#[derive(Debug)]
pub struct MetricsListener {
    /// Fired to request a graceful stop.
    stop: oneshot::Sender<()>,
    /// The serving task.
    task: JoinHandle<Result<(), kynos::Error>>,
    /// The address actually bound.
    address: SocketAddr,
}

impl MetricsListener {
    /// Binds `address` and serves `exporter` on it in a background task. The
    /// bind happens before this returns, so a taken or invalid address fails
    /// the caller (the server's boot) rather than the task.
    ///
    /// # Errors
    ///
    /// Returns an error if the service cannot be built or the address cannot
    /// be bound.
    pub async fn start(address: &str, exporter: Exporter) -> eyre::Result<Self> {
        let (stop, stopped) = oneshot::channel::<()>();
        let bound = Server::new(service(exporter)?)
            .bind(address)
            .graceful_shutdown(Shutdown::on(async move {
                // A dropped sender is also a stop request.
                let _ = stopped.await;
            }))
            .shutdown_timeout(SHUTDOWN_TIMEOUT)
            .prepare()
            .await?;
        let address = bound
            .local_addrs()
            .first()
            .copied()
            .ok_or_else(|| eyre::eyre!("the metrics listener bound no address at {address}"))?;
        tracing::info!(%address, path = METRICS_PATH, "metrics listener started");
        let task = tokio::spawn(bound.serve());
        Ok(Self {
            stop,
            task,
            address,
        })
    }

    /// The address the listener is bound to.
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    /// Stops the listener and waits for in-flight scrapes to drain.
    pub async fn shutdown(self) {
        // A send error only means the task already exited.
        let _ = self.stop.send(());
        match self.task.await {
            Ok(Ok(())) => {}
            Ok(Err(kynos::Error::Server(error))) if error.is_requested_shutdown() => {}
            Ok(Err(error)) => tracing::error!(%error, "metrics listener failed"),
            Err(error) => tracing::error!(%error, "metrics listener ended abnormally"),
        }
        tracing::info!("metrics listener stopped");
    }
}
