//! The shared rate-limit counters over real PostgreSQL, gated on
//! `CRISP_TEST_DATABASE_URL` like `tests/postgres.rs`: two replicas sharing
//! one database cannot jointly exceed a budget, the refill, `Retry-After`,
//! and key isolation match the in-process limiter, the purge deletes only
//! idle counters, and a lost database fails the check closed.

mod common;
#[allow(dead_code)] // Only the gated connection is used here.
mod pg;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use common::{MockStorage, platform, test_config};
use crisp_server::limits::{Budget, Decision, Limiter, Limits, Operation, PgLimiter, RateLimits};
use crisp_server::state::App;
use crisp_server::storage::Storage;
use kynos::http::StatusCode;
use kynos::test::{TestClient, TestResponse};
use pg::connect;
use serde_json::json;

/// `requests` per `period`.
fn budget(requests: u32, period: Duration) -> Budget {
    Budget::new(NonZeroU32::new(requests).unwrap(), period)
}

/// A key no other test or run has spent from.
fn fresh_key(label: &str) -> String {
    format!("{label}-{}", hex::encode(rand::random::<[u8; 8]>()))
}

/// A second, independent pool on the test database, as another replica
/// would hold.
async fn replica(test: &str) -> PgLimiter {
    PgLimiter::new(connect(test).await.expect("the first connect succeeded"))
}

/// One replica's router: mock storage, but rate-limit counters in the
/// shared database, allowing `requests` challenges per username per hour.
fn replica_app(limiter: PgLimiter, requests: u32) -> TestClient<App> {
    let mut config = test_config();
    config.http.limits.challenge = budget(requests, Duration::from_secs(3600));
    // The in-process client has no address, so every run shares the peer
    // key `unknown`; keep that budget out of the way.
    config.http.limits.challenge_peer = budget(10_000, Duration::from_secs(60));
    let storage = Arc::new(MockStorage::default()) as Arc<dyn Storage>;
    let app = App::new(storage, Arc::new(platform()), Arc::new(config.clone()))
        .with_limiter(Arc::new(limiter));
    TestClient::new(
        crisp_server::router(&config.http)
            .build(app)
            .expect("router builds"),
    )
}

/// Asks `client` for a login challenge for `username`.
async fn challenge(client: &TestClient<App>, username: &str) -> TestResponse {
    client
        .post("/api/v1/auth/challenge")
        .json(&json!({ "username": username }))
        .send()
        .await
}

#[tokio::test]
async fn two_replicas_share_one_budget() {
    let Some(db) = connect("two_replicas_share_one_budget").await else {
        return;
    };
    let first = replica_app(PgLimiter::new(db), 3);
    let second = replica_app(replica("two_replicas_share_one_budget").await, 3);
    let username = fresh_key("u");
    // The username is unregistered, so an allowed challenge is a decoy
    // answer (200), and the fourth, on either replica, is refused.
    for client in [&first, &second, &first] {
        challenge(client, &username)
            .await
            .assert_status(StatusCode::OK);
    }
    for client in [&second, &first] {
        let refused = challenge(client, &username).await;
        refused.assert_status(StatusCode::TOO_MANY_REQUESTS);
        let retry_after: u64 = refused
            .header("retry-after")
            .expect("a Retry-After header")
            .parse()
            .expect("whole seconds");
        // One unit refills every 20 minutes.
        assert!((1..=1200).contains(&retry_after), "{retry_after}");
    }
}

#[tokio::test]
async fn concurrent_spends_on_two_replicas_never_exceed_the_budget() {
    let Some(db) = connect("concurrent_spends_on_two_replicas").await else {
        return;
    };
    let limiters: [Arc<dyn Limiter>; 2] = [
        Arc::new(PgLimiter::new(db)),
        Arc::new(replica("concurrent_spends_on_two_replicas").await),
    ];
    let key = fresh_key("burst");
    let b = budget(5, Duration::from_secs(3600));
    let spends = (0..24).map(|i| {
        let limiter = Arc::clone(&limiters[i % 2]);
        let key = key.clone();
        tokio::spawn(async move { limiter.spend(Operation::Publish, &key, b).await })
    });
    let mut allowed = 0;
    for spend in spends.collect::<Vec<_>>() {
        if spend.await.unwrap().expect("spend") == Decision::Allowed {
            allowed += 1;
        }
    }
    assert_eq!(allowed, 5, "exactly the budget is spent across replicas");
}

#[tokio::test]
async fn the_allowance_refills_and_keys_are_isolated() {
    let Some(db) = connect("the_allowance_refills_and_keys_are_isolated").await else {
        return;
    };
    let limiter = PgLimiter::new(db);
    // 2 per 2 s: a unit every second.
    let b = budget(2, Duration::from_secs(2));
    let key = fresh_key("refill");
    for _ in 0..2 {
        assert_eq!(
            limiter.spend(Operation::Login, &key, b).await.unwrap(),
            Decision::Allowed
        );
    }
    let Decision::Refused { wait } = limiter.spend(Operation::Login, &key, b).await.unwrap() else {
        panic!("the third spend is refused");
    };
    assert!(wait <= Duration::from_secs(1), "{wait:?}");
    // Another key, and the same key under another operation, are untouched.
    let other = fresh_key("other");
    assert_eq!(
        limiter.spend(Operation::Login, &other, b).await.unwrap(),
        Decision::Allowed
    );
    assert_eq!(
        limiter.spend(Operation::Register, &key, b).await.unwrap(),
        Decision::Allowed
    );
    tokio::time::sleep(wait + Duration::from_millis(50)).await;
    assert_eq!(
        limiter.spend(Operation::Login, &key, b).await.unwrap(),
        Decision::Allowed
    );
    assert!(matches!(
        limiter.spend(Operation::Login, &key, b).await.unwrap(),
        Decision::Refused { .. }
    ));
}

#[tokio::test]
async fn the_retry_after_is_one_refill_interval() {
    let Some(db) = connect("the_retry_after_is_one_refill_interval").await else {
        return;
    };
    let budgets = RateLimits {
        login: budget(3, Duration::from_secs(60)),
        ..RateLimits::default()
    };
    let limits = Limits::new(budgets, Arc::new(PgLimiter::new(db)));
    let key = fresh_key("retry");
    for _ in 0..3 {
        limits.check(Operation::Login, &key).await.unwrap();
    }
    match limits.check(Operation::Login, &key).await {
        // One unit refills every 20 s, less the few milliseconds the
        // spends above took, rounded up.
        Err(crisp_server::limits::Refusal::Limited(error)) => {
            assert_eq!(error.retry_after_secs(), 20);
        }
        other => panic!("expected a 429 refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_purge_deletes_only_idle_counters() {
    let Some(db) = connect("a_purge_deletes_only_idle_counters").await else {
        return;
    };
    let limiter = PgLimiter::new(db);
    let idle = fresh_key("idle");
    let live = fresh_key("live");
    limiter
        .spend(
            Operation::Query,
            &idle,
            budget(1, Duration::from_millis(200)),
        )
        .await
        .unwrap();
    limiter
        .spend(
            Operation::Query,
            &live,
            budget(1, Duration::from_secs(3600)),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Two replicas purging at once delete the idle row once between them.
    let second = replica("a_purge_deletes_only_idle_counters").await;
    let (a, b) = tokio::join!(limiter.purge(), second.purge());
    assert!(a.unwrap() + b.unwrap() >= 1);
    // The idle key is fresh again; the live one is still spent.
    let hour = budget(1, Duration::from_secs(3600));
    assert_eq!(
        limiter.spend(Operation::Query, &idle, hour).await.unwrap(),
        Decision::Allowed
    );
    assert!(matches!(
        limiter.spend(Operation::Query, &live, hour).await.unwrap(),
        Decision::Refused { .. }
    ));
}

#[tokio::test]
async fn a_lost_database_fails_the_check_closed() {
    let Some(db) = connect("a_lost_database_fails_the_check_closed").await else {
        return;
    };
    let limits = Limits::new(RateLimits::default(), Arc::new(PgLimiter::new(db.clone())));
    db.close().await.expect("close");
    assert!(matches!(
        limits.check(Operation::Login, "k").await,
        Err(crisp_server::limits::Refusal::Unavailable(_))
    ));
}
