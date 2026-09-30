//! Time that keeps passing while the computer sleeps.
//!
//! On Linux and macOS, the monotonic clock stops while the computer sleeps.
//! This means that [`tokio::time::sleep`] may not always sleep for the exact
//! duration requested.
//! This approach uses both a monotonic and wall clock to ensure the wait survives
//! the computer sleeping and the user changing the computer's clock.

use std::time::{Duration, SystemTime};
use tokio::time::Instant;

const MAX_STEP: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy)]
pub struct Moment {
    pub instant: Instant,
    pub wall: SystemTime,
}

impl Moment {
    pub fn now() -> Self {
        Self {
            instant: Instant::now(),
            wall: SystemTime::now(),
        }
    }

    pub fn checked_add(self, duration: Duration) -> Option<Self> {
        Some(Self {
            instant: self.instant.checked_add(duration)?,
            wall: self.wall.checked_add(duration)?,
        })
    }

    pub fn remaining(self, now: Moment) -> Duration {
        let monotonic = self.instant.saturating_duration_since(now.instant);
        let wall = self.wall.duration_since(now.wall).unwrap_or_default();
        monotonic.min(wall)
    }

    pub fn has_passed(self, now: Moment) -> bool {
        self.remaining(now).is_zero()
    }
}

/// Completes after `duration`. Just like [`tokio::time::sleep`] but counting the
/// time the computer is asleep.
pub async fn after(duration: Duration) {
    let Some(deadline) = Moment::now().checked_add(duration) else {
        return std::future::pending().await;
    };

    loop {
        let remaining = deadline.remaining(Moment::now());
        if remaining.is_zero() {
            return;
        }
        tokio::time::sleep(remaining.min(MAX_STEP)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn check_remaining(awake: u64, wall: i64, expected: u64) {
        let start = Moment::now();
        let deadline = start.checked_add(Duration::from_secs(30)).unwrap();
        let offset = Duration::from_secs(wall.unsigned_abs());
        let now = Moment {
            instant: start.instant + Duration::from_secs(awake),
            wall: match wall >= 0 {
                true => start.wall + offset,
                false => start.wall - offset,
            },
        };

        assert_eq!(deadline.remaining(now), Duration::from_secs(expected));
        assert_eq!(deadline.has_passed(now), expected == 0);
    }

    #[test]
    fn deadline_passes_once_both_clocks_reach_it() {
        check_remaining(0, 0, 30);
        check_remaining(29, 29, 1);
        check_remaining(30, 30, 0);
        check_remaining(3600, 3600, 0);
    }

    #[test]
    fn sleeping_counts_towards_the_deadline() {
        check_remaining(1, 29, 1);
        check_remaining(1, 3600, 0);
    }

    #[test]
    fn setting_the_clock_back_does_not_delay_the_deadline() {
        check_remaining(29, -3600, 1);
        check_remaining(30, -3600, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn after_waits_for_the_full_duration() {
        let start = Instant::now();
        after(Duration::from_secs(95)).await;
        assert_eq!(start.elapsed(), Duration::from_secs(95));
    }
}
