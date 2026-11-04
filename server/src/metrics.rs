//! Operational metrics in the Prometheus/OpenMetrics model.
//!
//! [`Metrics`] owns one registry and every metric family the server records:
//!
//! - `crisp_http_requests_total{operation, status}` — every response, by the
//!   OpenAPI `operationId` it answered (`<unmatched>` for none) and its
//!   status code, so request and error rates are one query each;
//! - `crisp_http_request_duration_seconds{operation}` — time to produce the
//!   response head;
//! - `crisp_cpu_job_duration_seconds{job}` and
//!   `crisp_cpu_job_wait_seconds{job}` — how long signature verification and
//!   endorsement (`publish`, `register`, `login`) ran on the bounded CPU pool,
//!   and how long each waited for a slot;
//! - `crisp_purge_runs_total{outcome}` and `crisp_purged_rows_total{kind}` —
//!   the background purge's passes and what they deleted;
//! - `crisp_db_pool_connections`, `crisp_db_pool_idle_connections`, and
//!   `crisp_db_pool_max_connections` — the PostgreSQL pool, sampled at each
//!   scrape.
//!
//! Every label value is drawn from a fixed set, so no series is keyed by
//! request data. The registry is served by [`exporter`] on its own listener,
//! never on the public API.

pub mod exporter;

use std::sync::Arc;
use std::time::Duration;

use kynos::http;
use kynos::middleware::Observer;
use kynos::router::operation::Route;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{Histogram, exponential_buckets};
use prometheus_client::registry::Registry;

use crate::purge::PurgeOutcome;

/// The `operation` label of a request no operation matched.
pub const UNMATCHED: &str = "<unmatched>";

/// The labels of `crisp_http_requests_total`.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct RequestLabels {
    operation: String,
    status: String,
}

/// The labels of `crisp_http_request_duration_seconds`.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct OperationLabels {
    operation: String,
}

/// The labels of the CPU-pool histograms.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct JobLabels {
    job: &'static str,
}

/// The labels of `crisp_purge_runs_total`.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct OutcomeLabels {
    outcome: &'static str,
}

/// The labels of `crisp_purged_rows_total`.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct KindLabels {
    kind: &'static str,
}

/// A histogram family whose buckets are fixed by a constructor.
type Histograms<L> = Family<L, Histogram, fn() -> Histogram>;

/// Latency buckets from 1 ms to about 16 s, doubling.
fn latency_histogram() -> Histogram {
    Histogram::new(exponential_buckets(0.001, 2.0, 15))
}

/// The PostgreSQL connection pool's occupancy at one instant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PoolUsage {
    /// Connections currently open, idle or in use.
    pub open: u32,
    /// Open connections not checked out by a request.
    pub idle: u32,
    /// The most connections the pool will open.
    pub max: u32,
}

/// Every metric family, behind one [`Arc`].
#[derive(Debug)]
struct Families {
    registry: Registry,
    requests: Family<RequestLabels, Counter>,
    latency: Histograms<OperationLabels>,
    cpu_run: Histograms<JobLabels>,
    cpu_wait: Histograms<JobLabels>,
    purge_runs: Family<OutcomeLabels, Counter>,
    purged_rows: Family<KindLabels, Counter>,
    pool_open: Gauge,
    pool_idle: Gauge,
    pool_max: Gauge,
}

/// The server's metric registry; cloning shares it.
#[derive(Clone, Debug)]
pub struct Metrics(Arc<Families>);

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// A registry holding every family, with no samples yet.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Registry::with_prefix("crisp");
        let requests = Family::<RequestLabels, Counter>::default();
        let latency: Histograms<OperationLabels> = Family::new_with_constructor(latency_histogram);
        let cpu_run: Histograms<JobLabels> = Family::new_with_constructor(latency_histogram);
        let cpu_wait: Histograms<JobLabels> = Family::new_with_constructor(latency_histogram);
        let purge_runs = Family::<OutcomeLabels, Counter>::default();
        let purged_rows = Family::<KindLabels, Counter>::default();
        let (pool_open, pool_idle, pool_max) =
            (Gauge::default(), Gauge::default(), Gauge::default());
        registry.register(
            "http_requests",
            "Responses sent, by operation id and status code",
            requests.clone(),
        );
        registry.register(
            "http_request_duration_seconds",
            "Time to produce a response head, by operation id",
            latency.clone(),
        );
        registry.register(
            "cpu_job_duration_seconds",
            "Time a verification or endorsement job ran on the CPU pool, by job",
            cpu_run.clone(),
        );
        registry.register(
            "cpu_job_wait_seconds",
            "Time a job waited for a CPU pool slot, by job",
            cpu_wait.clone(),
        );
        registry.register(
            "purge_runs",
            "Background purge passes, by outcome (ok or error)",
            purge_runs.clone(),
        );
        registry.register(
            "purged_rows",
            "Expired rows the purge deleted, by kind (session or challenge)",
            purged_rows.clone(),
        );
        registry.register(
            "db_pool_connections",
            "Open PostgreSQL connections, idle or in use",
            pool_open.clone(),
        );
        registry.register(
            "db_pool_idle_connections",
            "Open PostgreSQL connections not in use",
            pool_idle.clone(),
        );
        registry.register(
            "db_pool_max_connections",
            "The PostgreSQL pool's connection ceiling",
            pool_max.clone(),
        );
        // The purge's series exist from boot at zero, so a rate or an alert
        // on them has a baseline before the first pass.
        for outcome in ["ok", "error"] {
            let _ = purge_runs.get_or_create(&OutcomeLabels { outcome });
        }
        for kind in ["session", "challenge"] {
            let _ = purged_rows.get_or_create(&KindLabels { kind });
        }
        Self(Arc::new(Families {
            registry,
            requests,
            latency,
            cpu_run,
            cpu_wait,
            purge_runs,
            purged_rows,
            pool_open,
            pool_idle,
            pool_max,
        }))
    }

    /// Records one response to `operation` with `status`, produced in
    /// `elapsed`.
    pub fn record_request(&self, operation: &str, status: u16, elapsed: Duration) {
        self.0
            .requests
            .get_or_create(&RequestLabels {
                operation: operation.to_owned(),
                status: status.to_string(),
            })
            .inc();
        self.0
            .latency
            .get_or_create(&OperationLabels {
                operation: operation.to_owned(),
            })
            .observe(elapsed.as_secs_f64());
    }

    /// Records one CPU-pool `job` that waited `waited` for a slot and then ran
    /// for `ran`.
    pub fn record_cpu_job(&self, job: &'static str, waited: Duration, ran: Duration) {
        let labels = JobLabels { job };
        self.0
            .cpu_wait
            .get_or_create(&labels)
            .observe(waited.as_secs_f64());
        self.0
            .cpu_run
            .get_or_create(&labels)
            .observe(ran.as_secs_f64());
    }

    /// Records one purge pass.
    pub fn record_purge(&self, outcome: &PurgeOutcome) {
        let label = match outcome {
            PurgeOutcome::Purged(counts) => {
                for (kind, rows) in [
                    ("session", counts.sessions),
                    ("challenge", counts.challenges),
                ] {
                    self.0
                        .purged_rows
                        .get_or_create(&KindLabels { kind })
                        .inc_by(rows);
                }
                "ok"
            }
            PurgeOutcome::Failed => "error",
        };
        self.0
            .purge_runs
            .get_or_create(&OutcomeLabels { outcome: label })
            .inc();
    }

    /// Sets the pool gauges to `usage`.
    pub fn record_pool(&self, usage: PoolUsage) {
        self.0.pool_open.set(i64::from(usage.open));
        self.0.pool_idle.set(i64::from(usage.idle));
        self.0.pool_max.set(i64::from(usage.max));
    }

    /// The registry as an OpenMetrics text exposition.
    ///
    /// # Errors
    ///
    /// Returns [`std::fmt::Error`] if a metric fails to encode.
    pub fn encode(&self) -> Result<String, std::fmt::Error> {
        let mut text = String::new();
        prometheus_client::encoding::text::encode(&mut text, &self.0.registry)?;
        Ok(text)
    }

    /// The observer that records every response the router sends.
    #[must_use]
    pub fn observer(&self) -> RequestMetrics {
        RequestMetrics(self.clone())
    }
}

/// A router [`Observer`] recording request counts and latencies into
/// [`Metrics`].
#[derive(Clone, Debug)]
pub struct RequestMetrics(Metrics);

impl<C> Observer<C> for RequestMetrics {
    fn on_request(&self, _request: &http::Request, _route: Option<Route<'_>>, _context: &C) {}

    fn on_response(&self, response: &http::Response, route: Option<Route<'_>>, elapsed: Duration) {
        let operation = route.map_or(UNMATCHED, |route| route.operation_id());
        self.0
            .record_request(operation, response.status().as_u16(), elapsed);
    }
}

#[cfg(test)]
mod tests;
