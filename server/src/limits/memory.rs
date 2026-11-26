//! The in-process [`Limiter`]: the same [`gcra`] arithmetic as
//! [`super::PgLimiter`], on counters in this process and an injectable
//! [`Clock`]. Its counters are not shared between processes, so it serves
//! tests and a server context built without a database, never a deployment.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use sea_orm::DbErr;

use super::{Budget, Decision, Limiter, Operation, gcra};

/// Once the limiter tracks this many keys, keys whose state is
/// indistinguishable from fresh are dropped before the next spend. The next
/// sweep then waits until the live count doubles, so sweeping stays amortised
/// O(1) per request even when every tracked key is still active.
pub(super) const MAX_TRACKED_KEYS: usize = 10_000;

/// A source of the current time, in microseconds since the Unix epoch.
pub trait Clock: Send + Sync + 'static {
    /// The current time in microseconds since the Unix epoch.
    fn now_micros(&self) -> i64;
}

/// The system wall clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_micros(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_micros()).unwrap_or(i64::MAX)
            })
    }
}

/// A clock that moves only when told to, for deterministic tests. Clones
/// share one time.
#[derive(Debug, Clone, Default)]
pub struct ManualClock(Arc<AtomicI64>);

impl ManualClock {
    /// Moves the clock forward by `by`.
    pub fn advance(&self, by: Duration) {
        let micros = i64::try_from(by.as_micros()).unwrap_or(i64::MAX);
        self.0.fetch_add(micros, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_micros(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// The tracked keys and when the next sweep runs.
#[derive(Debug)]
struct Counters {
    /// Each key's theoretical arrival time, in microseconds.
    arrivals: HashMap<(Operation, String), i64>,
    /// The tracked-key count at which the next idle-key sweep runs.
    sweep_at: usize,
}

/// Rate-limit counters in this process's memory.
#[derive(Debug)]
pub struct MemoryLimiter<C: Clock = SystemClock> {
    /// The tracked keys.
    counters: Mutex<Counters>,
    /// The time source.
    clock: C,
}

impl MemoryLimiter {
    /// Fresh counters on the system clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(SystemClock)
    }
}

impl Default for MemoryLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Clock> MemoryLimiter<C> {
    /// Fresh counters on `clock`.
    #[must_use]
    pub fn with_clock(clock: C) -> Self {
        Self {
            counters: Mutex::new(Counters {
                arrivals: HashMap::new(),
                sweep_at: MAX_TRACKED_KEYS,
            }),
            clock,
        }
    }

    /// How many keys are tracked.
    #[must_use]
    pub fn tracked(&self) -> usize {
        self.lock().arrivals.len()
    }

    /// The counters; a panic while they were held left them consistent
    /// (every write is a single insert), so a poisoned lock is recovered.
    fn lock(&self) -> std::sync::MutexGuard<'_, Counters> {
        self.counters.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Spends one request synchronously; [`Limiter::spend`] wraps it.
    fn spend_now(&self, operation: Operation, key: &str, budget: Budget) -> Decision {
        let now = self.clock.now_micros();
        let mut counters = self.lock();
        if counters.arrivals.len() >= counters.sweep_at {
            counters.arrivals.retain(|_, tat| *tat > now);
            let tracked = counters.arrivals.len();
            counters.sweep_at = MAX_TRACKED_KEYS.max(tracked * 2);
            tracing::debug!(tracked, "evicted idle rate-limit keys");
        }
        let slot = (operation, key.to_owned());
        let step = gcra(budget, counters.arrivals.get(&slot).copied(), now);
        if let Ok(tat) = step {
            counters.arrivals.insert(slot, tat);
        }
        Decision::from_gcra(step)
    }

    /// The sweep threshold, for the eviction tests.
    #[cfg(test)]
    pub(super) fn sweep_at(&self) -> usize {
        self.lock().sweep_at
    }
}

// See `Limiter`: the `#[must_use]` is `async_trait`'s.
#[allow(clippy::double_must_use)]
#[async_trait]
impl<C: Clock> Limiter for MemoryLimiter<C> {
    async fn spend(
        &self,
        operation: Operation,
        key: &str,
        budget: Budget,
    ) -> Result<Decision, DbErr> {
        Ok(self.spend_now(operation, key, budget))
    }

    async fn purge(&self) -> Result<u64, DbErr> {
        let now = self.clock.now_micros();
        let mut counters = self.lock();
        let before = counters.arrivals.len();
        counters.arrivals.retain(|_, tat| *tat > now);
        Ok(u64::try_from(before - counters.arrivals.len()).unwrap_or(u64::MAX))
    }
}
