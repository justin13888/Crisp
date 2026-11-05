//! The background purge of expired sessions and login challenges.
//!
//! Expired rows are already ignored by every read, so purging is hygiene, not
//! correctness: it bounds table growth. The task runs on a fixed interval and
//! stops promptly when its shutdown signal fires, so it never outlives the
//! server.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::metrics::Metrics;
use crate::storage::{PurgeCounts, Storage};
use crate::util::now_epoch;

/// A running purge task and the handle that stops it.
#[derive(Debug)]
pub struct Purger {
    /// Flipped to `true` to request a stop.
    stop: watch::Sender<bool>,
    /// The task itself.
    task: JoinHandle<()>,
}

impl Purger {
    /// Spawns the purge loop on the current Tokio runtime. The first pass runs
    /// immediately, then one every `interval`; each pass is recorded into
    /// `metrics`.
    #[must_use]
    pub fn spawn(storage: Arc<dyn Storage>, interval: Duration, metrics: Metrics) -> Self {
        let (stop, stopped) = watch::channel(false);
        let task = tokio::spawn(run(storage, interval, metrics, stopped));
        tracing::info!(interval_secs = interval.as_secs(), "purge task started");
        Self { stop, task }
    }

    /// Signals the loop to stop and waits for it to finish.
    pub async fn shutdown(self) {
        // A send error only means the task already exited.
        let _ = self.stop.send(true);
        if let Err(error) = self.task.await {
            tracing::error!(%error, "purge task ended abnormally");
        }
        tracing::info!("purge task stopped");
    }
}

/// The purge loop: one pass per tick until `stopped` flips.
async fn run(
    storage: Arc<dyn Storage>,
    interval: Duration,
    metrics: Metrics,
    mut stopped: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            changed = stopped.changed() => {
                // A dropped sender is also a stop request.
                if changed.is_err() || *stopped.borrow() {
                    return;
                }
            }
            _ = ticker.tick() => metrics.record_purge(&purge_once(storage.as_ref()).await),
        }
    }
}

/// What one purge pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurgeOutcome {
    /// The pass ran and deleted these rows (possibly none).
    Purged(PurgeCounts),
    /// The pass failed; the error was logged.
    Failed,
}

/// Runs one purge pass, logging (never propagating) failures so a transient
/// database error does not kill the loop, and returns what it did.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn purge_once(storage: &dyn Storage) -> PurgeOutcome {
    match storage.purge_expired(now_epoch()).await {
        Ok(counts) => {
            if counts.is_empty() {
                tracing::debug!("purge found nothing expired");
            } else {
                tracing::info!(
                    sessions = counts.sessions,
                    challenges = counts.challenges,
                    "purged expired rows"
                );
            }
            PurgeOutcome::Purged(counts)
        }
        Err(error) => {
            tracing::error!(%error, "purge of expired rows failed");
            PurgeOutcome::Failed
        }
    }
}
