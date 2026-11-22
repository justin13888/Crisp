//! The time source, behind a trait so tests are deterministic.

use crisp_core::model::primitives::Timestamp;

/// A source of the current time.
pub trait Clock: Send + Sync {
    /// Now, in seconds since the Unix epoch.
    fn now(&self) -> Timestamp;
}

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        )
    }
}

/// A clock stopped at one instant.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_tell_the_time() {
        assert!(SystemClock.now().0 > 1_700_000_000);
        assert_eq!(FixedClock(Timestamp(5)).now(), Timestamp(5));
    }
}
