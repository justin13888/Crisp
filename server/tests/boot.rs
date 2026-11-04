//! Boot tests of the `crisp-server` binary: a database that cannot be
//! reached at startup fails the boot fast with a non-zero exit, leaving the
//! retry to the supervisor's restart policy rather than serving without one.

use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn boot_fails_fast_when_the_database_is_unreachable() {
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_crisp-server"))
        // Port 1 is reserved and nothing listens on it, so the connect is refused.
        .env("CRISP_DATABASE_URL", "postgresql://crisp@127.0.0.1:1/crisp")
        .env("CRISP_PLATFORM_SEED", "2a".repeat(32))
        .env("CRISP_DB_ACQUIRE_TIMEOUT_SECS", "1")
        .env("CRISP_BIND_ADDR", "127.0.0.1:0")
        .output()
        .expect("the server binary runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "boot must fail without a database; stderr: {stderr}"
    );
    // The configuration loaded, so the failure is the database connect.
    assert!(
        stderr.contains("Connection Error"),
        "boot must reach the database connect; stderr: {stderr}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "boot must fail fast, not retry; took {:?}",
        started.elapsed()
    );
}
