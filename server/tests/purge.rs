//! The background purge of expired sessions and challenges, over
//! `MockStorage`.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{Fault, MockStorage, now};
use crisp_server::db::challenge;
use crisp_server::metrics::Metrics;
use crisp_server::purge::{PurgeOutcome, Purger, purge_once};
use crisp_server::storage::{PurgeCounts, Storage};

#[tokio::test]
async fn purge_once_removes_only_expired_rows() {
    let storage = MockStorage::default();
    storage.put_session("old", "usr1a", now() - 10);
    storage.put_session("new", "usr1a", now() + 600);
    storage
        .inner
        .lock()
        .unwrap()
        .challenges
        .push(challenge::Model {
            nonce: "n".into(),
            usr_id: "usr1a".into(),
            created_at: 0,
            expires_at: now() - 1,
        });

    let outcome = purge_once(&storage).await;

    assert_eq!(
        outcome,
        PurgeOutcome::Purged(PurgeCounts {
            sessions: 1,
            challenges: 1
        })
    );
    assert!(!storage.has_session("old"));
    assert!(storage.has_session("new"));
    assert!(storage.inner.lock().unwrap().challenges.is_empty());
}

#[tokio::test]
async fn purge_failures_are_logged_not_fatal() {
    let storage = MockStorage::default();
    storage.inject("purge_expired", Fault::Db);
    assert_eq!(purge_once(&storage).await, PurgeOutcome::Failed); // must not panic
    assert!(storage.purge_expired(now()).await.is_err());
}

#[tokio::test]
async fn purger_runs_on_its_interval_and_stops_on_shutdown() {
    let storage = Arc::new(MockStorage::default());
    storage.put_session("old", "usr1a", now() - 10);

    let metrics = Metrics::new();
    let purger = Purger::spawn(
        Arc::clone(&storage) as Arc<dyn Storage>,
        Duration::from_millis(10),
        metrics.clone(),
    );
    // The first pass is immediate; wait for a few more.
    tokio::time::timeout(Duration::from_secs(5), async {
        while storage.inner.lock().unwrap().purges < 3 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the purger ticks");
    assert!(!storage.has_session("old"));

    tokio::time::timeout(Duration::from_secs(5), purger.shutdown())
        .await
        .expect("the purger stops promptly on shutdown");
    let after = storage.inner.lock().unwrap().purges;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        storage.inner.lock().unwrap().purges,
        after,
        "no passes after shutdown"
    );
    // Every pass the loop ran is recorded, and the one expired session too.
    let text = metrics.encode().unwrap();
    assert!(
        text.contains(&format!(
            "crisp_purge_runs_total{{outcome=\"ok\"}} {after}\n"
        )),
        "{text}"
    );
    assert!(
        text.contains("crisp_purged_rows_total{kind=\"session\"} 1\n"),
        "{text}"
    );
}

#[tokio::test]
async fn a_failing_pass_is_recorded_as_an_error_run() {
    let storage = Arc::new(MockStorage::default());
    storage.inject("purge_expired", Fault::Db);
    let metrics = Metrics::new();
    let purger = Purger::spawn(
        Arc::clone(&storage) as Arc<dyn Storage>,
        Duration::from_secs(3600),
        metrics.clone(),
    );
    // The first pass runs immediately.
    tokio::time::timeout(Duration::from_secs(5), async {
        while !metrics
            .encode()
            .unwrap()
            .contains("crisp_purge_runs_total{outcome=\"error\"} 1\n")
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the failed pass is recorded");
    purger.shutdown().await;
}
