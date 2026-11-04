//! The application context threaded through every handler.
//!
//! [`App`] is the kynos dependency-injection context: `#[derive(Provider)]`
//! lets a handler ask for any field by type with `Inject<T>`. Each field is a
//! cheap-to-clone newtype over an [`Arc`], so injection is a refcount bump,
//! and each wraps a distinct type so lookup-by-type is unambiguous.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Instant;

use kynos::Provider;
use tokio::sync::Semaphore;

use crate::auth::SessionAuthenticator;
use crate::config::Config;
use crate::error::Internal;
use crate::limits::{Limits, Throttle};
use crate::platform::PlatformKey;
use crate::storage::Storage;

/// The persistence layer, as a trait object so tests can substitute an
/// in-memory implementation without touching a database.
#[derive(Clone)]
pub struct Store(pub Arc<dyn Storage>);

impl std::ops::Deref for Store {
    type Target = dyn Storage;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// The platform endorsement key.
#[derive(Clone, Debug)]
pub struct Platform(pub Arc<PlatformKey>);

impl std::ops::Deref for Platform {
    type Target = PlatformKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Immutable runtime configuration.
#[derive(Clone, Debug)]
pub struct Settings(pub Arc<Config>);

impl std::ops::Deref for Settings {
    type Target = Config;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The bounded pool that CPU-heavy request work runs on.
///
/// Signature verification, endorsement, and bundle encoding take
/// milliseconds per call and grow with the upload, so running them on an
/// async worker would stall every other request scheduled there. [`run`]
/// moves the work onto Tokio's blocking threads, and a semaphore caps how
/// many such jobs run at once (`CRISP_VERIFY_CONCURRENCY`): callers past the
/// cap wait for a permit, holding no thread while they do.
///
/// [`run`]: CpuPool::run
#[derive(Clone, Debug)]
pub struct CpuPool(Arc<Semaphore>);

impl CpuPool {
    /// A pool running at most `bound` jobs at once.
    #[must_use]
    pub fn new(bound: NonZeroUsize) -> Self {
        Self(Arc::new(Semaphore::new(bound.get())))
    }

    /// How many more jobs could start right now without waiting.
    #[must_use]
    pub fn available(&self) -> usize {
        self.0.available_permits()
    }

    /// Runs `work` on a blocking thread once a permit is free, and returns
    /// its result. `job` names the work in the logs.
    ///
    /// # Errors
    ///
    /// Returns [`Internal`] if `work` panics or the pool is shut down.
    pub async fn run<T, F>(&self, job: &'static str, work: F) -> Result<T, Internal>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let queued = Instant::now();
        let permit = Arc::clone(&self.0)
            .acquire_owned()
            .await
            .map_err(|e| Internal::logged("acquiring a cpu pool permit", &e))?;
        let waited_us = queued.elapsed().as_micros();
        let span = tracing::Span::current();
        let outcome = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _entered = span.enter();
            let started = Instant::now();
            let output = work();
            tracing::debug!(
                job,
                waited_us,
                ran_us = started.elapsed().as_micros(),
                "cpu pool job finished"
            );
            output
        })
        .await;
        outcome.map_err(|e| Internal::logged("running a cpu pool job", &e))
    }
}

/// The context every request handler is served with.
#[derive(Provider)]
pub struct App {
    /// The persistence layer.
    pub storage: Store,
    /// The platform endorsement signing key.
    pub platform: Platform,
    /// Immutable runtime configuration.
    pub settings: Settings,
    /// The per-operation rate-limit counters.
    pub throttle: Throttle,
    /// The bounded pool verification and endorsement run on.
    pub cpu: CpuPool,
    /// Resolves bearer tokens to accounts; reached through
    /// [`kynos::security::Authenticates`], not injection.
    #[provide(skip)]
    pub sessions: SessionAuthenticator,
}

impl App {
    /// Assembles the context from its parts, with fresh rate-limit counters
    /// enforcing `config.http.limits` and a [`CpuPool`] bounded by
    /// `config.http.verify_concurrency`.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>, platform: Arc<PlatformKey>, config: Arc<Config>) -> Self {
        let throttle = Throttle(Arc::new(Limits::new(&config.http.limits)));
        let cpu = CpuPool::new(config.http.verify_concurrency);
        Self {
            storage: Store(storage),
            platform: Platform(platform),
            settings: Settings(config),
            throttle,
            cpu,
            sessions: SessionAuthenticator,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn pool(bound: usize) -> CpuPool {
        CpuPool::new(NonZeroUsize::new(bound).unwrap())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn blocked_jobs_leave_the_async_worker_free() {
        let cpu = pool(1);
        let (release, gate) = mpsc::channel::<()>();
        let job = tokio::spawn({
            let cpu = cpu.clone();
            async move { cpu.run("test", move || gate.recv().is_ok()).await }
        });
        // The only async worker keeps serving while the job is blocked.
        while cpu.available() > 0 {
            tokio::task::yield_now().await;
        }
        assert_eq!(cpu.available(), 0);
        release.send(()).unwrap();
        assert!(job.await.unwrap().unwrap());
        assert_eq!(cpu.available(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_bound_caps_concurrent_jobs() {
        let cpu = pool(2);
        let running = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let peak = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let jobs = (0..8).map(|_| {
            let (cpu, running, peak) = (cpu.clone(), Arc::clone(&running), Arc::clone(&peak));
            tokio::spawn(async move {
                cpu.run("test", move || {
                    use std::sync::atomic::Ordering::SeqCst;
                    let now = running.fetch_add(1, SeqCst) + 1;
                    peak.fetch_max(now, SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    running.fetch_sub(1, SeqCst);
                })
                .await
            })
        });
        for job in jobs.collect::<Vec<_>>() {
            job.await.unwrap().unwrap();
        }
        let peak = peak.load(std::sync::atomic::Ordering::SeqCst);
        assert!((1..=2).contains(&peak), "peak {peak} exceeds the bound");
        assert_eq!(cpu.available(), 2);
    }

    #[tokio::test]
    async fn a_panicking_job_is_an_internal_error_and_frees_its_permit() {
        let cpu = pool(1);
        let result = cpu.run("test", || panic!("boom")).await;
        assert!(result.is_err());
        assert_eq!(cpu.available(), 1);
    }
}
