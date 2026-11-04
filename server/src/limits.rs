//! Per-endpoint abuse limits.
//!
//! Each throttled operation spends from a [`Budget`]: so many requests per
//! period, per key, enforced in-process by a keyed GCRA limiter. A request
//! over budget is refused with a `429` [`TooManyRequests`] carrying
//! `Retry-After` before the handler does any expensive work.
//!
//! | operation                              | key              | default    |
//! |----------------------------------------|------------------|------------|
//! | `POST /auth/register`                  | client IP        | 10 / hour  |
//! | `POST /auth/challenge`                 | username         | 10 / 5 min |
//! | `POST /auth/challenge` (also)          | client IP        | 30 / min   |
//! | `POST /auth/login`                     | client IP        | 30 / min   |
//! | `POST /bundles`                        | account `usr…`   | 60 / hour  |
//! | `GET /recipes`, `GET /search` (shared) | client IP        | 120 / min  |
//!
//! Every other operation is a single indexed lookup or a static answer and is
//! not throttled. The client IP is the TCP peer unless that peer is a trusted
//! proxy (`CRISP_TRUSTED_PROXIES`), in which case it is the address the proxy
//! wrote into its forwarding header ([`client_key`]). With no proxy trusted,
//! a reverse proxy's clients all share its budget: a coarse but unspoofable
//! bound.
//!
//! Counters live in this process, so a restart forgets them and replicas do
//! not share them.

use std::borrow::Cow;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use governor::clock::{Clock, DefaultClock};
use governor::middleware::NoOpMiddleware;
use governor::state::keyed::DashMapStateStore;
use governor::{Quota, RateLimiter};
use kynos::http::forwarded::Forwarded;
use sha2::{Digest, Sha256};

use crate::error::TooManyRequests;

/// Once a limiter tracks this many keys, keys whose state is indistinguishable
/// from fresh are dropped before the next check. The next sweep then waits
/// until the live count doubles, so sweeping stays amortised O(1) per request
/// even when every tracked key is still active.
const MAX_TRACKED_KEYS: usize = 10_000;

/// A key longer than this (a username nobody registered can be any length)
/// is counted under its SHA-256 digest, bounding each tracked key's size.
const MAX_KEY_BYTES: usize = 64;

/// `key`, or its hex SHA-256 digest when longer than [`MAX_KEY_BYTES`].
fn bounded_key(key: &str) -> Cow<'_, str> {
    if key.len() > MAX_KEY_BYTES {
        Cow::Owned(hex::encode(Sha256::digest(key.as_bytes())))
    } else {
        Cow::Borrowed(key)
    }
}

/// The rate-limit key for the client a request came from: its address as
/// the trusted-proxy policy resolved it, or `unknown` where nothing named one
/// (an in-process request, or a trusted proxy that wrote no address).
#[must_use]
pub fn client_key(forwarded: &Forwarded) -> String {
    forwarded
        .client()
        .map_or_else(|| "unknown".to_owned(), |address| address.to_string())
}

/// A throttled operation; each has its own [`Budget`] and counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    /// `POST /auth/register`, keyed by client IP.
    Register,
    /// `POST /auth/challenge`, keyed by username.
    Challenge,
    /// `POST /auth/challenge` again, keyed by client IP (bounds username
    /// rotation).
    ChallengePeer,
    /// `POST /auth/login`, keyed by client IP.
    Login,
    /// `POST /bundles`, keyed by account.
    Publish,
    /// `GET /recipes` and `GET /search` together, keyed by client IP.
    Query,
}

impl Operation {
    /// The name the operation is logged under.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::Challenge => "challenge",
            Self::ChallengePeer => "challenge_peer",
            Self::Login => "login",
            Self::Publish => "publish",
            Self::Query => "query",
        }
    }
}

/// How many requests one key may make per period.
///
/// The full allowance may be spent at once; it then refills evenly, one
/// request every `period / requests`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// The requests allowed per period (and the largest burst).
    pub requests: NonZeroU32,
    /// The period the allowance refills over.
    pub period: Duration,
}

impl Budget {
    /// `requests` per `period`.
    #[must_use]
    pub const fn new(requests: NonZeroU32, period: Duration) -> Self {
        Self { requests, period }
    }

    /// The governor quota enforcing this budget. A refill interval that
    /// rounds to zero is raised to one nanosecond.
    fn quota(self) -> Quota {
        let interval = (self.period / self.requests.get()).max(Duration::from_nanos(1));
        Quota::with_period(interval)
            .unwrap_or_else(|| Quota::per_second(self.requests))
            .allow_burst(self.requests)
    }
}

/// A [`Budget`] from a literal request count; a zero count becomes one.
const fn budget(requests: u32, period_secs: u64) -> Budget {
    let requests = match NonZeroU32::new(requests) {
        Some(requests) => requests,
        None => NonZeroU32::MIN,
    };
    Budget::new(requests, Duration::from_secs(period_secs))
}

/// The budget of every throttled [`Operation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimits {
    /// `POST /auth/register` (`CRISP_LIMIT_REGISTER`).
    pub register: Budget,
    /// `POST /auth/challenge` (`CRISP_LIMIT_CHALLENGE`).
    pub challenge: Budget,
    /// `POST /auth/challenge` per peer (`CRISP_LIMIT_CHALLENGE_PEER`).
    pub challenge_peer: Budget,
    /// `POST /auth/login` (`CRISP_LIMIT_LOGIN`).
    pub login: Budget,
    /// `POST /bundles` (`CRISP_LIMIT_PUBLISH`).
    pub publish: Budget,
    /// `GET /recipes` and `GET /search` (`CRISP_LIMIT_QUERY`).
    pub query: Budget,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self {
            register: budget(10, 60 * 60),
            challenge: budget(10, 5 * 60),
            challenge_peer: budget(30, 60),
            login: budget(30, 60),
            publish: budget(60, 60 * 60),
            query: budget(120, 60),
        }
    }
}

impl RateLimits {
    /// The budget of `operation`.
    #[must_use]
    pub const fn of(&self, operation: Operation) -> Budget {
        match operation {
            Operation::Register => self.register,
            Operation::Challenge => self.challenge,
            Operation::ChallengePeer => self.challenge_peer,
            Operation::Login => self.login,
            Operation::Publish => self.publish,
            Operation::Query => self.query,
        }
    }
}

/// One operation's keyed limiter.
struct Bucket<C: Clock> {
    limiter: RateLimiter<String, DashMapStateStore<String>, C, NoOpMiddleware<C::Instant>>,
    /// The tracked-key count at which the next idle-key sweep runs.
    sweep_at: AtomicUsize,
}

impl<C: Clock> Bucket<C> {
    fn new(budget: Budget, clock: C) -> Self {
        Self {
            limiter: RateLimiter::dashmap_with_clock(budget.quota(), clock),
            sweep_at: AtomicUsize::new(MAX_TRACKED_KEYS),
        }
    }

    /// Spends one request for `key`, or says how long until one is allowed.
    fn spend(&self, key: &str) -> Result<(), Duration> {
        if self.limiter.len() >= self.sweep_at.load(Ordering::Relaxed) {
            self.limiter.retain_recent();
            let tracked = self.limiter.len();
            self.sweep_at
                .store(MAX_TRACKED_KEYS.max(tracked * 2), Ordering::Relaxed);
            tracing::debug!(tracked, "evicted idle rate-limit keys");
        }
        self.limiter
            .check_key(&key.to_owned())
            .map_err(|not_until| not_until.wait_time_from(self.limiter.clock().now()))
    }
}

/// The counters of every throttled operation, shared by all requests.
pub struct Limits<C: Clock = DefaultClock> {
    register: Bucket<C>,
    challenge: Bucket<C>,
    challenge_peer: Bucket<C>,
    login: Bucket<C>,
    publish: Bucket<C>,
    query: Bucket<C>,
}

impl Limits {
    /// Fresh counters enforcing `budgets` on the system clock.
    #[must_use]
    pub fn new(budgets: &RateLimits) -> Self {
        Self::with_clock(budgets, &DefaultClock::default())
    }
}

impl<C: Clock> Limits<C> {
    /// Fresh counters enforcing `budgets` on `clock`.
    #[must_use]
    pub fn with_clock(budgets: &RateLimits, clock: &C) -> Self
    where
        C: Clone,
    {
        let bucket = |operation| Bucket::new(budgets.of(operation), clock.clone());
        Self {
            register: bucket(Operation::Register),
            challenge: bucket(Operation::Challenge),
            challenge_peer: bucket(Operation::ChallengePeer),
            login: bucket(Operation::Login),
            publish: bucket(Operation::Publish),
            query: bucket(Operation::Query),
        }
    }

    /// Spends one request of `operation`'s budget for `key`.
    ///
    /// # Errors
    ///
    /// Returns [`TooManyRequests`] if `key` has exhausted the budget; the
    /// refused request spends nothing.
    pub fn check(&self, operation: Operation, key: &str) -> Result<(), TooManyRequests> {
        let bucket = match operation {
            Operation::Register => &self.register,
            Operation::Challenge => &self.challenge,
            Operation::ChallengePeer => &self.challenge_peer,
            Operation::Login => &self.login,
            Operation::Publish => &self.publish,
            Operation::Query => &self.query,
        };
        let key = bounded_key(key);
        let key = key.as_ref();
        bucket.spend(key).map_err(|wait| {
            let error = TooManyRequests::new(operation.name(), wait);
            tracing::info!(
                operation = operation.name(),
                key,
                retry_after_secs = error.retry_after_secs(),
                "rate limit exceeded"
            );
            error
        })?;
        tracing::trace!(operation = operation.name(), key, "rate limit spent");
        Ok(())
    }
}

/// The shared [`Limits`], injectable into handlers.
#[derive(Clone)]
pub struct Throttle(pub Arc<Limits>);

impl std::ops::Deref for Throttle {
    type Target = Limits;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests;
