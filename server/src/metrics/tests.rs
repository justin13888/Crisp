use super::*;
use crate::storage::PurgeCounts;

fn sample_line<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.lines().find(|line| line.starts_with(prefix))
}

#[test]
fn a_fresh_registry_holds_the_purge_baselines_and_pool_gauges() {
    let text = Metrics::new().encode().unwrap();
    for sample in [
        "crisp_purge_runs_total{outcome=\"ok\"} 0\n",
        "crisp_purge_runs_total{outcome=\"error\"} 0\n",
        "crisp_purged_rows_total{kind=\"session\"} 0\n",
        "crisp_purged_rows_total{kind=\"challenge\"} 0\n",
        "crisp_db_pool_connections 0\n",
        "crisp_db_pool_idle_connections 0\n",
        "crisp_db_pool_max_connections 0\n",
    ] {
        assert!(text.contains(sample), "{sample}:\n{text}");
    }
    // The request and job series appear once their first sample is
    // recorded.
    assert!(!text.contains("crisp_http_requests_total{"), "{text}");
    assert!(text.ends_with("# EOF\n"), "{text}");
}

#[test]
fn requests_are_counted_by_operation_and_status_with_latency() {
    let metrics = Metrics::new();
    metrics.record_request("search", 200, Duration::from_millis(3));
    metrics.record_request("search", 200, Duration::from_millis(5));
    metrics.record_request("search", 503, Duration::from_millis(1));
    let text = metrics.encode().unwrap();
    assert_eq!(
        sample_line(
            &text,
            r#"crisp_http_requests_total{operation="search",status="200"}"#
        ),
        Some(r#"crisp_http_requests_total{operation="search",status="200"} 2"#)
    );
    assert_eq!(
        sample_line(
            &text,
            r#"crisp_http_requests_total{operation="search",status="503"}"#
        ),
        Some(r#"crisp_http_requests_total{operation="search",status="503"} 1"#)
    );
    assert_eq!(
        sample_line(
            &text,
            r#"crisp_http_request_duration_seconds_count{operation="search"}"#
        ),
        Some(r#"crisp_http_request_duration_seconds_count{operation="search"} 3"#)
    );
    // 3 ms falls in the 4 ms bucket, not the 2 ms one.
    assert!(
        text.contains(
            r#"crisp_http_request_duration_seconds_bucket{le="0.002",operation="search"} 1"#
        ),
        "{text}"
    );
    assert!(
        text.contains(
            r#"crisp_http_request_duration_seconds_bucket{le="0.004",operation="search"} 2"#
        ),
        "{text}"
    );
}

#[test]
fn cpu_jobs_record_their_wait_and_run_time_by_job() {
    let metrics = Metrics::new();
    metrics.record_cpu_job(
        "publish",
        Duration::from_millis(2),
        Duration::from_millis(40),
    );
    let text = metrics.encode().unwrap();
    assert!(
        text.contains(r#"crisp_cpu_job_duration_seconds_sum{job="publish"} 0.04"#),
        "{text}"
    );
    assert!(
        text.contains(r#"crisp_cpu_job_wait_seconds_sum{job="publish"} 0.002"#),
        "{text}"
    );
    assert!(
        text.contains(r#"crisp_cpu_job_duration_seconds_count{job="publish"} 1"#),
        "{text}"
    );
}

#[test]
fn purges_count_their_outcome_and_the_rows_they_deleted() {
    let metrics = Metrics::new();
    metrics.record_purge(&PurgeOutcome::Purged(PurgeCounts {
        sessions: 3,
        challenges: 2,
    }));
    metrics.record_purge(&PurgeOutcome::Purged(PurgeCounts {
        sessions: 1,
        challenges: 0,
    }));
    metrics.record_purge(&PurgeOutcome::Failed);
    let text = metrics.encode().unwrap();
    assert!(
        text.contains(r#"crisp_purge_runs_total{outcome="ok"} 2"#),
        "{text}"
    );
    assert!(
        text.contains(r#"crisp_purge_runs_total{outcome="error"} 1"#),
        "{text}"
    );
    assert!(
        text.contains(r#"crisp_purged_rows_total{kind="session"} 4"#),
        "{text}"
    );
    assert!(
        text.contains(r#"crisp_purged_rows_total{kind="challenge"} 2"#),
        "{text}"
    );
}

#[test]
fn pool_gauges_hold_the_latest_sample() {
    let metrics = Metrics::new();
    metrics.record_pool(PoolUsage {
        open: 9,
        idle: 1,
        max: 10,
    });
    metrics.record_pool(PoolUsage {
        open: 4,
        idle: 3,
        max: 10,
    });
    let text = metrics.encode().unwrap();
    assert!(text.contains("crisp_db_pool_connections 4\n"), "{text}");
    assert!(
        text.contains("crisp_db_pool_idle_connections 3\n"),
        "{text}"
    );
    assert!(
        text.contains("crisp_db_pool_max_connections 10\n"),
        "{text}"
    );
}

#[test]
fn clones_share_one_registry() {
    let metrics = Metrics::new();
    metrics
        .clone()
        .record_request("health", 200, Duration::ZERO);
    let text = metrics.encode().unwrap();
    assert!(
        text.contains(r#"crisp_http_requests_total{operation="health",status="200"} 1"#),
        "{text}"
    );
}
