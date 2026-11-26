use std::sync::atomic::{AtomicUsize, Ordering};

use super::memory::MAX_TRACKED_KEYS;
use super::*;

const OPERATIONS: [Operation; 7] = [
    Operation::Register,
    Operation::Challenge,
    Operation::ChallengePeer,
    Operation::Login,
    Operation::Publish,
    Operation::Suggest,
    Operation::Query,
];

/// Every operation allows `requests` per `secs`.
fn uniform(requests: u32, secs: u64) -> RateLimits {
    let b = budget(requests, secs);
    RateLimits {
        register: b,
        challenge: b,
        challenge_peer: b,
        login: b,
        publish: b,
        suggest: b,
        query: b,
    }
}

/// Limits enforcing `budgets` on in-process counters on `clock`.
fn limits(budgets: RateLimits, clock: &ManualClock) -> Limits {
    Limits::new(budgets, Arc::new(MemoryLimiter::with_clock(clock.clone())))
}

/// The `Retry-After` of a refusal, panicking on anything else.
fn refused(outcome: Result<(), Refusal>) -> u64 {
    match outcome {
        Err(Refusal::Limited(error)) => error.retry_after_secs(),
        other => panic!("expected a 429 refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_budget_allows_its_burst_then_refuses_with_the_refill_wait() {
    let clock = ManualClock::default();
    let limits = limits(uniform(3, 60), &clock);
    for operation in OPERATIONS {
        for _ in 0..3 {
            limits.check(operation, "k").await.unwrap();
        }
        // One request refills every 20 s.
        assert_eq!(
            refused(limits.check(operation, "k").await),
            20,
            "{operation:?}"
        );
    }
}

#[tokio::test]
async fn the_allowance_refills_over_the_period() {
    let clock = ManualClock::default();
    let limits = limits(uniform(2, 10), &clock);
    limits.check(Operation::Login, "k").await.unwrap();
    limits.check(Operation::Login, "k").await.unwrap();
    assert!(limits.check(Operation::Login, "k").await.is_err());
    clock.advance(Duration::from_secs(4));
    assert_eq!(refused(limits.check(Operation::Login, "k").await), 1);
    clock.advance(Duration::from_secs(1));
    limits.check(Operation::Login, "k").await.unwrap();
    assert!(limits.check(Operation::Login, "k").await.is_err());
}

#[tokio::test]
async fn keys_and_operations_have_separate_counters() {
    let clock = ManualClock::default();
    let limits = limits(uniform(1, 60), &clock);
    limits.check(Operation::Register, "a").await.unwrap();
    assert!(limits.check(Operation::Register, "a").await.is_err());
    limits.check(Operation::Register, "b").await.unwrap();
    for operation in &OPERATIONS[1..] {
        limits.check(*operation, "a").await.unwrap();
    }
}

/// A limiter whose every call fails, as a database outage would.
struct Down {
    calls: AtomicUsize,
}

#[async_trait]
impl Limiter for Down {
    async fn spend(&self, _: Operation, _: &str, _: Budget) -> Result<Decision, DbErr> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(DbErr::Custom("connection refused".to_owned()))
    }

    async fn purge(&self) -> Result<u64, DbErr> {
        Err(DbErr::Custom("connection refused".to_owned()))
    }
}

#[tokio::test]
async fn a_failing_limiter_fails_closed_as_unavailable() {
    let down = Arc::new(Down {
        calls: AtomicUsize::new(0),
    });
    let limits = Limits::new(uniform(10, 60), Arc::clone(&down) as Arc<dyn Limiter>);
    let outcome = limits.check(Operation::Publish, "usr1").await;
    assert!(
        matches!(outcome, Err(Refusal::Unavailable(_))),
        "{outcome:?}"
    );
    assert_eq!(down.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn idle_keys_are_evicted_once_the_limiter_is_full() {
    let clock = ManualClock::default();
    let limiter = MemoryLimiter::with_clock(clock.clone());
    let b = budget(1, 1);
    for key in 0..MAX_TRACKED_KEYS {
        let decision = limiter.spend(Operation::Query, &key.to_string(), b).await;
        assert_eq!(decision.unwrap(), Decision::Allowed);
    }
    assert_eq!(limiter.tracked(), MAX_TRACKED_KEYS);
    clock.advance(Duration::from_secs(2));
    limiter.spend(Operation::Query, "fresh", b).await.unwrap();
    assert_eq!(limiter.tracked(), 1);
}

#[tokio::test]
async fn keys_below_the_cap_are_kept() {
    let clock = ManualClock::default();
    let limiter = MemoryLimiter::with_clock(clock.clone());
    let b = budget(1, 1);
    for key in 0..MAX_TRACKED_KEYS - 1 {
        limiter
            .spend(Operation::Query, &key.to_string(), b)
            .await
            .unwrap();
    }
    clock.advance(Duration::from_secs(2));
    limiter.spend(Operation::Query, "fresh", b).await.unwrap();
    assert_eq!(limiter.tracked(), MAX_TRACKED_KEYS);
}

#[tokio::test]
async fn a_sweep_that_frees_nothing_waits_for_the_count_to_double() {
    let limiter = MemoryLimiter::with_clock(ManualClock::default());
    let b = budget(1, 60);
    for key in 0..=MAX_TRACKED_KEYS {
        limiter
            .spend(Operation::Query, &key.to_string(), b)
            .await
            .unwrap();
    }
    assert_eq!(limiter.sweep_at(), 2 * MAX_TRACKED_KEYS);
    assert_eq!(limiter.tracked(), MAX_TRACKED_KEYS + 1);
}

#[tokio::test]
async fn a_purge_deletes_only_idle_counters() {
    let clock = ManualClock::default();
    let limiter = MemoryLimiter::with_clock(clock.clone());
    limiter
        .spend(Operation::Login, "short", budget(1, 1))
        .await
        .unwrap();
    limiter
        .spend(Operation::Login, "long", budget(1, 60))
        .await
        .unwrap();
    assert_eq!(limiter.purge().await.unwrap(), 0);
    clock.advance(Duration::from_secs(2));
    assert_eq!(limiter.purge().await.unwrap(), 1);
    assert_eq!(limiter.tracked(), 1);
}

#[tokio::test]
async fn long_keys_are_counted_under_their_digest() {
    let short = "a".repeat(MAX_KEY_BYTES);
    assert_eq!(bounded_key(&short), short.as_str());
    let long = "a".repeat(MAX_KEY_BYTES + 1);
    let digest = bounded_key(&long);
    assert_eq!(digest.len(), 64);
    assert_ne!(digest, bounded_key(&"b".repeat(MAX_KEY_BYTES + 1)));

    let clock = ManualClock::default();
    let limits = limits(uniform(1, 60), &clock);
    limits.check(Operation::Challenge, &long).await.unwrap();
    assert!(limits.check(Operation::Challenge, &long).await.is_err());
    assert!(limits.check(Operation::Challenge, &digest).await.is_err());
}

#[test]
fn a_tiny_refill_interval_still_enforces_the_burst() {
    let b = Budget::new(NonZeroU32::MAX, Duration::from_secs(1));
    assert_eq!(b.interval_micros(), 1);
    assert_eq!(b.tolerance_micros(), 999_999);
    assert!(gcra(b, None, 0).is_ok());
}

#[test]
fn the_gcra_step_spends_a_burst_then_waits_one_interval() {
    // 3 per minute: a unit every 20 s, a tolerance of 40 s.
    let b = budget(3, 60);
    let second = 1_000_000;
    assert_eq!(b.interval_micros(), 20 * second);
    assert_eq!(b.tolerance_micros(), 40 * second);
    let first = gcra(b, None, 0).unwrap();
    assert_eq!(first, 20 * second);
    let second_tat = gcra(b, Some(first), 0).unwrap();
    let third_tat = gcra(b, Some(second_tat), 0).unwrap();
    assert_eq!(third_tat, 60 * second);
    assert_eq!(gcra(b, Some(third_tat), 0), Err(20 * second));
    // A key idle past its arrival time is as good as fresh.
    assert_eq!(gcra(b, Some(third_tat), 100 * second), Ok(120 * second));
}

#[test]
fn a_decision_carries_the_wait_of_a_refusal() {
    assert_eq!(Decision::from_gcra(Ok(5)), Decision::Allowed);
    assert_eq!(
        Decision::from_gcra::<()>(Err(1_500_000)),
        Decision::Refused {
            wait: Duration::from_millis(1500)
        }
    );
}

#[test]
fn budget_of_selects_each_operation() {
    let limits = RateLimits {
        register: budget(1, 1),
        challenge: budget(2, 1),
        challenge_peer: budget(6, 1),
        login: budget(3, 1),
        publish: budget(4, 1),
        suggest: budget(7, 1),
        query: budget(5, 1),
    };
    let counts: Vec<u32> = OPERATIONS
        .iter()
        .map(|operation| limits.of(*operation).requests.get())
        .collect();
    assert_eq!(counts, [1, 2, 6, 3, 4, 7, 5]);
    assert_eq!(budget(0, 1).requests.get(), 1);
}

#[test]
fn default_budgets_match_the_documented_table() {
    let defaults = RateLimits::default();
    let table: Vec<(u32, u64)> = OPERATIONS
        .iter()
        .map(|operation| {
            let budget = defaults.of(*operation);
            (budget.requests.get(), budget.period.as_secs())
        })
        .collect();
    assert_eq!(
        table,
        [
            (10, 3600),
            (10, 300),
            (30, 60),
            (30, 60),
            (60, 3600),
            (20, 3600),
            (120, 60)
        ]
    );
}

#[test]
fn operations_have_distinct_names() {
    let names: std::collections::HashSet<_> = OPERATIONS
        .iter()
        .map(|operation| operation.name())
        .collect();
    assert_eq!(names.len(), OPERATIONS.len());
}
