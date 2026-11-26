//! Per-endpoint abuse limits.
//!
//! Each throttled operation spends from a [`Budget`]: so many requests per
//! period, per key, enforced by a keyed GCRA limiter ([`gcra`]). A request
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
//! | `POST /ingredients/suggestions`        | account `usr…`   | 20 / hour  |
//! | `GET /recipes`, `GET /search` (shared) | client IP        | 120 / min  |
//!
//! Every other operation is a single indexed lookup or a static answer and is
//! not throttled. The client IP is the TCP peer unless that peer is a trusted
//! proxy (`CRISP_TRUSTED_PROXIES`), in which case it is the address the proxy
//! wrote into its forwarding header ([`client_key`]). With no proxy trusted,
//! a reverse proxy's clients all share its budget: a coarse but unspoofable
//! bound.
//!
//! The counters live behind the [`Limiter`] trait. The server keeps them in
//! PostgreSQL ([`PgLimiter`]): every replica behind one ingress spends from
//! the same row with one atomic statement on the database clock, so N
//! replicas never multiply a budget and a restart forgets nothing. The
//! in-process [`MemoryLimiter`] runs the same arithmetic on an injectable
//! clock, for tests and for a server context built without a database.
//!
//! A limiter that cannot answer (the database is down or too slow) fails
//! closed: the request is refused `503` [`ServiceUnavailable`] and the
//! failure is logged at ERROR, so an outage never lifts the budgets.

use std::borrow::Cow;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use kynos::http::forwarded::Forwarded;
use sea_orm::DbErr;
use sha2::{Digest, Sha256};

use crate::error::{ServiceUnavailable, TooManyRequests};

mod memory;
mod postgres;

pub use memory::{Clock, ManualClock, MemoryLimiter, SystemClock};
pub use postgres::PgLimiter;

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
    /// `POST /ingredients/suggestions`, keyed by account.
    Suggest,
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
            Self::Suggest => "suggest",
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

    /// The period in microseconds, saturating at `i64::MAX`.
    fn period_micros(self) -> i64 {
        i64::try_from(self.period.as_micros()).unwrap_or(i64::MAX)
    }

    /// The refill interval in microseconds: the time one request's
    /// allowance takes to come back. An interval that rounds to zero is
    /// raised to one microsecond.
    #[must_use]
    pub fn interval_micros(self) -> i64 {
        (self.period_micros() / i64::from(self.requests.get())).max(1)
    }

    /// The GCRA tolerance in microseconds: how far ahead of the clock a
    /// key's theoretical arrival time may run and still be allowed, which
    /// is what lets the whole allowance be spent at once.
    #[must_use]
    pub fn tolerance_micros(self) -> i64 {
        (self.period_micros() - self.interval_micros()).max(0)
    }
}

/// One GCRA step, the arithmetic both limiters share: a key whose
/// theoretical arrival time is `tat` (`None` for a key never seen) asks at
/// `now`, all in microseconds.
///
/// Returns the key's new arrival time when the request is allowed, or the
/// microseconds until it would be when it is refused; a refusal leaves `tat`
/// unchanged.
///
/// # Errors
///
/// The wait, in microseconds, when `budget` is exhausted.
pub fn gcra(budget: Budget, tat: Option<i64>, now: i64) -> Result<i64, i64> {
    let base = tat.map_or(now, |tat| tat.max(now));
    let ahead = base.saturating_sub(now);
    let tolerance = budget.tolerance_micros();
    if ahead > tolerance {
        Err(ahead - tolerance)
    } else {
        Ok(base.saturating_add(budget.interval_micros()))
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
    /// `POST /ingredients/suggestions` (`CRISP_LIMIT_SUGGEST`).
    pub suggest: Budget,
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
            suggest: budget(20, 60 * 60),
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
            Operation::Suggest => self.suggest,
            Operation::Query => self.query,
        }
    }
}

/// What a [`Limiter`] decided for one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The request was allowed and spent one unit of the budget.
    Allowed,
    /// The budget is exhausted; one request is allowed again after `wait`.
    /// The refused request spent nothing.
    Refused {
        /// How long until one request is allowed.
        wait: Duration,
    },
}

impl Decision {
    /// The decision a [`gcra`] step's outcome stands for.
    #[must_use]
    pub fn from_gcra<T>(step: Result<T, i64>) -> Self {
        match step {
            Ok(_) => Self::Allowed,
            Err(wait) => Self::Refused {
                wait: Duration::from_micros(u64::try_from(wait).unwrap_or(0)),
            },
        }
    }
}

/// Where the rate-limit counters live: the mockable boundary of the
/// limiter, so handler tests run on a fake clock with no database.
// `async_trait` marks each boxed-future method `#[must_use]`, which clippy then
// flags as redundant on a `Result`-returning method; the attribute is not ours.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait Limiter: Send + Sync + 'static {
    /// Spends one request of `budget` for `key` under `operation`, or
    /// refuses it without spending, atomically with respect to every other
    /// caller sharing these counters. `key` is already bounded.
    async fn spend(
        &self,
        operation: Operation,
        key: &str,
        budget: Budget,
    ) -> Result<Decision, DbErr>;

    /// Deletes every counter indistinguishable from a fresh one, so the
    /// counters stay bounded, and returns how many it deleted. Safe to run
    /// on several replicas at once.
    async fn purge(&self) -> Result<u64, DbErr>;
}

/// Why [`Limits::check`] refused a request.
#[derive(Debug)]
pub enum Refusal {
    /// The key exhausted the operation's budget (`429`).
    Limited(TooManyRequests),
    /// The counters could not be read or written (`503`); the request is
    /// refused rather than let through unmetered.
    Unavailable(ServiceUnavailable),
}

/// The budgets of every throttled operation and the [`Limiter`] that keeps
/// their counters, shared by all requests.
pub struct Limits {
    /// The budget of each operation.
    budgets: RateLimits,
    /// Where the counters live.
    limiter: Arc<dyn Limiter>,
}

impl Limits {
    /// Enforces `budgets` with the counters `limiter` keeps.
    #[must_use]
    pub fn new(budgets: RateLimits, limiter: Arc<dyn Limiter>) -> Self {
        Self { budgets, limiter }
    }

    /// The limiter keeping the counters.
    #[must_use]
    pub fn limiter(&self) -> &Arc<dyn Limiter> {
        &self.limiter
    }

    /// Spends one request of `operation`'s budget for `key`.
    ///
    /// # Errors
    ///
    /// [`Refusal::Limited`] if `key` has exhausted the budget (the refused
    /// request spends nothing), or [`Refusal::Unavailable`] if the limiter
    /// failed: the check fails closed.
    #[tracing::instrument(level = "trace", skip_all, fields(operation = operation.name()))]
    pub async fn check(&self, operation: Operation, key: &str) -> Result<(), Refusal> {
        let key = bounded_key(key);
        let key = key.as_ref();
        let budget = self.budgets.of(operation);
        match self.limiter.spend(operation, key, budget).await {
            Ok(Decision::Allowed) => {
                tracing::trace!(operation = operation.name(), key, "rate limit spent");
                Ok(())
            }
            Ok(Decision::Refused { wait }) => {
                let error = TooManyRequests::new(operation.name(), wait);
                tracing::info!(
                    operation = operation.name(),
                    key,
                    retry_after_secs = error.retry_after_secs(),
                    "rate limit exceeded"
                );
                Err(Refusal::Limited(error))
            }
            Err(error) => {
                tracing::error!(
                    operation = operation.name(),
                    key,
                    %error,
                    "rate limit check failed; refusing the request"
                );
                Err(Refusal::Unavailable(ServiceUnavailable))
            }
        }
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
