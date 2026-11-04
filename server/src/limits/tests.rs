use governor::clock::FakeRelativeClock;

use super::*;

const OPERATIONS: [Operation; 6] = [
    Operation::Register,
    Operation::Challenge,
    Operation::ChallengePeer,
    Operation::Login,
    Operation::Publish,
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
        query: b,
    }
}

#[test]
fn a_budget_allows_its_burst_then_refuses_with_the_refill_wait() {
    let clock = FakeRelativeClock::default();
    let limits = Limits::with_clock(&uniform(3, 60), &clock);
    for operation in OPERATIONS {
        for _ in 0..3 {
            limits.check(operation, "k").unwrap();
        }
        let refused = limits.check(operation, "k").unwrap_err();
        // One request refills every 20 s.
        assert_eq!(refused.retry_after_secs(), 20, "{operation:?}");
    }
}

#[test]
fn the_allowance_refills_over_the_period() {
    let clock = FakeRelativeClock::default();
    let limits = Limits::with_clock(&uniform(2, 10), &clock);
    limits.check(Operation::Login, "k").unwrap();
    limits.check(Operation::Login, "k").unwrap();
    assert!(limits.check(Operation::Login, "k").is_err());
    clock.advance(Duration::from_secs(4));
    let refused = limits.check(Operation::Login, "k").unwrap_err();
    assert_eq!(refused.retry_after_secs(), 1);
    clock.advance(Duration::from_secs(1));
    limits.check(Operation::Login, "k").unwrap();
    assert!(limits.check(Operation::Login, "k").is_err());
}

#[test]
fn keys_and_operations_have_separate_counters() {
    let clock = FakeRelativeClock::default();
    let limits = Limits::with_clock(&uniform(1, 60), &clock);
    limits.check(Operation::Register, "a").unwrap();
    assert!(limits.check(Operation::Register, "a").is_err());
    limits.check(Operation::Register, "b").unwrap();
    for operation in &OPERATIONS[1..] {
        limits.check(*operation, "a").unwrap();
    }
}

#[test]
fn idle_keys_are_evicted_once_the_limiter_is_full() {
    let clock = FakeRelativeClock::default();
    let bucket = Bucket::new(budget(1, 1), clock.clone());
    for key in 0..MAX_TRACKED_KEYS {
        bucket.spend(&key.to_string()).unwrap();
    }
    assert_eq!(bucket.limiter.len(), MAX_TRACKED_KEYS);
    clock.advance(Duration::from_secs(2));
    bucket.spend("fresh").unwrap();
    assert_eq!(bucket.limiter.len(), 1);
}

#[test]
fn keys_below_the_cap_are_kept() {
    let clock = FakeRelativeClock::default();
    let bucket = Bucket::new(budget(1, 1), clock.clone());
    for key in 0..MAX_TRACKED_KEYS - 1 {
        bucket.spend(&key.to_string()).unwrap();
    }
    clock.advance(Duration::from_secs(2));
    bucket.spend("fresh").unwrap();
    assert_eq!(bucket.limiter.len(), MAX_TRACKED_KEYS);
}

#[test]
fn a_sweep_that_frees_nothing_waits_for_the_count_to_double() {
    let clock = FakeRelativeClock::default();
    let bucket = Bucket::new(budget(1, 60), clock);
    for key in 0..=MAX_TRACKED_KEYS {
        bucket.spend(&key.to_string()).unwrap();
    }
    assert_eq!(
        bucket.sweep_at.load(Ordering::Relaxed),
        2 * MAX_TRACKED_KEYS
    );
    assert_eq!(bucket.limiter.len(), MAX_TRACKED_KEYS + 1);
}

#[test]
fn long_keys_are_counted_under_their_digest() {
    let short = "a".repeat(MAX_KEY_BYTES);
    assert_eq!(bounded_key(&short), short.as_str());
    let long = "a".repeat(MAX_KEY_BYTES + 1);
    let digest = bounded_key(&long);
    assert_eq!(digest.len(), 64);
    assert_ne!(digest, bounded_key(&"b".repeat(MAX_KEY_BYTES + 1)));

    let clock = FakeRelativeClock::default();
    let limits = Limits::with_clock(&uniform(1, 60), &clock);
    limits.check(Operation::Challenge, &long).unwrap();
    assert!(limits.check(Operation::Challenge, &long).is_err());
    assert!(limits.check(Operation::Challenge, &digest).is_err());
}

#[test]
fn a_tiny_refill_interval_still_enforces_the_burst() {
    let clock = FakeRelativeClock::default();
    let bucket = Bucket::new(Budget::new(NonZeroU32::MAX, Duration::from_secs(1)), clock);
    assert!(bucket.spend("k").is_ok());
}

#[test]
fn budget_of_selects_each_operation() {
    let limits = RateLimits {
        register: budget(1, 1),
        challenge: budget(2, 1),
        challenge_peer: budget(6, 1),
        login: budget(3, 1),
        publish: budget(4, 1),
        query: budget(5, 1),
    };
    let counts: Vec<u32> = OPERATIONS
        .iter()
        .map(|operation| limits.of(*operation).requests.get())
        .collect();
    assert_eq!(counts, [1, 2, 6, 3, 4, 5]);
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
