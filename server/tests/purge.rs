//! The background purge of expired sessions and challenges, over
//! `MockStorage`.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{Fault, MockStorage, now};
use crisp_server::db::challenge;
use crisp_server::purge::{Purger, purge_once};
use crisp_server::storage::Storage;

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

    purge_once(&storage).await;

    assert!(!storage.has_session("old"));
    assert!(storage.has_session("new"));
    assert!(storage.inner.lock().unwrap().challenges.is_empty());
}

#[tokio::test]
async fn purge_failures_are_logged_not_fatal() {
    let storage = MockStorage::default();
    storage.inject("purge_expired", Fault::Db);
    purge_once(&storage).await; // must not panic
    assert!(storage.purge_expired(now()).await.is_err());
}

#[tokio::test]
async fn purger_runs_on_its_interval_and_stops_on_shutdown() {
    let storage = Arc::new(MockStorage::default());
    storage.put_session("old", "usr1a", now() - 10);

    let purger = Purger::spawn(
        Arc::clone(&storage) as Arc<dyn Storage>,
        Duration::from_millis(10),
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
}
