//! Small shared helpers: wall-clock time and random token generation.

use std::time::{SystemTime, UNIX_EPOCH};

use rand::RngCore;

/// The current Unix time, in whole seconds. Crisp timestamps are integer
/// epoch-seconds (spec §3.2), so the whole server speaks `i64` seconds.
#[must_use]
pub fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Generates a 256-bit random token, hex-encoded — used for session tokens
/// and login-challenge nonces.
#[must_use]
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_well_sized() {
        let a = random_token();
        let b = random_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn now_epoch_is_positive() {
        assert!(now_epoch() > 1_700_000_000);
    }
}
