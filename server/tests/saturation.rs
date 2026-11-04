//! Cheap requests stay responsive while publishes saturate the server.
//!
//! Publication verifies and endorses on the bounded CPU pool
//! (`crisp_server::state::CpuPool`), never on the async workers. These tests
//! run on a single-threaded runtime, so any CPU work done inline on the
//! worker would hold every other request until it finished.

mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use common::{Harness, RecipeSpec, build_recipe_bundle_with, test_config};
use crisp_core::author::{open_draft, seal};
use crisp_core::identity::derive_blob_id;
use crisp_core::model::blob::{Blob, MediaType};
use crisp_core::model::primitives::{OpenCode, Timestamp};
use crisp_core::sign::SecretKeySet;
use kynos::http::StatusCode;

const PUBLISHES: usize = 6;

#[tokio::test(flavor = "current_thread")]
async fn health_answers_while_publishes_are_verifying() {
    let mut config = test_config();
    config.http.verify_concurrency = NonZeroUsize::MIN;
    let h = Arc::new(Harness::with_config(config));
    let author = SecretKeySet::from_seed(&[7u8; 32]);
    let token = h.register_and_login(&author, "saturator").await;
    let bundles: Vec<Vec<u8>> = (0..PUBLISHES as u64)
        .map(|i| {
            let spec = RecipeSpec {
                modified_at: 2_000 + i,
                ..RecipeSpec::default()
            };
            build_recipe_bundle_with(&author, &spec).0
        })
        .collect();

    let done = Arc::new(AtomicUsize::new(0));
    let publishes: Vec<_> = bundles
        .into_iter()
        .map(|bytes| {
            let (h, token, done) = (Arc::clone(&h), token.clone(), Arc::clone(&done));
            tokio::spawn(async move {
                let status = h.publish(&token, bytes).await.status();
                done.fetch_add(1, Ordering::SeqCst);
                status
            })
        })
        .collect();
    // Let every publish start: each reaches the pool and parks there.
    for _ in 0..PUBLISHES {
        tokio::task::yield_now().await;
    }

    h.client
        .get("/api/v1/health")
        .send()
        .await
        .assert_status(StatusCode::OK);
    let finished = done.load(Ordering::SeqCst);
    assert!(
        finished < PUBLISHES,
        "health waited for all {finished} publishes to finish"
    );

    for publish in publishes {
        assert_eq!(publish.await.unwrap(), StatusCode::CREATED);
    }
}

/// A sealed recipe bundle carrying one `size`-byte image blob.
fn bundle_with_blob(author: &SecretKeySet, modified_at: u64, size: usize) -> Vec<u8> {
    let spec = RecipeSpec {
        modified_at,
        ..RecipeSpec::default()
    };
    let base = build_recipe_bundle_with(author, &spec).0;
    let mut draft = open_draft(&base).expect("open");
    let bytes = vec![0x5a; size];
    let id = derive_blob_id(&bytes).expect("blob id");
    draft.blobs.insert(
        id.clone(),
        Blob {
            media_type: OpenCode::Known(MediaType::Png),
            size: size as u64,
            width: None,
            height: None,
            bytes,
        },
    );
    draft.recipes[0].media = Some(vec![id]);
    seal(
        &draft,
        author,
        "Tester",
        Timestamp(modified_at + 1),
        Some(&base),
    )
    .expect("seal")
}

/// Publish latency against bundle size, through the real router. Run with
/// `cargo test --release -p crisp-server --test saturation -- --ignored
/// --nocapture`; the figures are recorded on the pull request that added it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "benchmark: run explicitly in a release build"]
async fn publish_latency_by_bundle_size() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[8u8; 32]);
    let token = h.register_and_login(&author, "bencher").await;
    // Sizes are blob payloads; the cap is just under 16 MiB so the whole
    // file, signatures included, fits the default upload limit.
    for (i, size) in [1usize << 10, 1 << 20, (16 << 20) - (64 << 10)]
        .into_iter()
        .enumerate()
    {
        let bytes = bundle_with_blob(&author, 3_000 + i as u64 * 10, size);
        let len = bytes.len();
        let started = Instant::now();
        let status = h.publish(&token, bytes).await.status();
        println!("publish {len} bytes -> {status} in {:?}", started.elapsed());
    }
}

/// `GET /health` latency while maximum-size publishes outnumber the async
/// workers. Run like [`publish_latency_by_bundle_size`].
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "benchmark: run explicitly in a release build"]
async fn health_latency_under_publish_load() {
    let h = Arc::new(Harness::new());
    let author = SecretKeySet::from_seed(&[9u8; 32]);
    let token = h.register_and_login(&author, "loader").await;
    let uploads: Vec<_> = (0..8u64)
        .map(|i| bundle_with_blob(&author, 4_000 + i * 10, (16 << 20) - (64 << 10)))
        .collect();
    let publishes: Vec<_> = uploads
        .into_iter()
        .map(|bytes| {
            let (h, token) = (Arc::clone(&h), token.clone());
            tokio::spawn(async move { h.publish(&token, bytes).await.status() })
        })
        .collect();
    // The probe is a spawned task too: the test body itself is driven off
    // the worker threads, so it would never see them stall. Each probe's
    // latency counts from when it was due, so time spent waiting for a
    // worker is included.
    let probe = tokio::spawn({
        let h = Arc::clone(&h);
        let due = Instant::now();
        async move {
            let mut worst = std::time::Duration::ZERO;
            let mut due = due;
            for _ in 0..50 {
                h.client.get("/api/v1/health").send().await;
                worst = worst.max(due.elapsed());
                tokio::task::yield_now().await;
                due = Instant::now();
            }
            worst
        }
    });
    let worst = probe.await.unwrap();
    let started = Instant::now();
    for publish in publishes {
        assert_eq!(publish.await.unwrap(), StatusCode::CREATED);
    }
    println!(
        "health worst of 50 under load: {worst:?}; publishes drained {:?} later",
        started.elapsed()
    );
}
