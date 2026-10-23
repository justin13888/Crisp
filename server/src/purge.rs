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

use crate::storage::Storage;
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
    /// immediately, then one every `interval`.
    #[must_use]
    pub fn spawn(storage: Arc<dyn Storage>, interval: Duration) -> Self {
        let (stop, stopped) = watch::channel(false);
        let task = tokio::spawn(run(storage, interval, stopped));
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
async fn run(storage: Arc<dyn Storage>, interval: Duration, mut stopped: watch::Receiver<bool>) {
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
            _ = ticker.tick() => purge_once(storage.as_ref()).await,
        }
    }
}

/// Runs one purge pass, logging (never propagating) failures so a transient
/// database error does not kill the loop.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn purge_once(storage: &dyn Storage) {
    match storage.purge_expired(now_epoch()).await {
        Ok(counts) if counts.sessions > 0 || counts.challenges > 0 => {
            tracing::info!(
                sessions = counts.sessions,
                challenges = counts.challenges,
                "purged expired rows"
            );
        }
        Ok(_) => tracing::debug!("purge found nothing expired"),
        Err(error) => tracing::error!(%error, "purge of expired rows failed"),
    }
}
