use std::{collections::VecDeque, time::Duration};
use tokio::time::Instant;

#[derive(Debug)]
pub(crate) struct Meter {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl Meter {
    pub(crate) fn new(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
        }
    }

    pub(crate) fn record(&mut self, at: Instant, total: u64) {
        self.samples.push_back((at, total));
        while let Some(&(oldest, _)) = self.samples.front()
            && at.saturating_duration_since(oldest) > self.window
        {
            self.samples.pop_front();
        }
    }

    pub(crate) fn rate(&self) -> Option<u64> {
        let (&(first_at, first), &(last_at, last)) = (self.samples.front()?, self.samples.back()?);
        let elapsed = last_at.saturating_duration_since(first_at).as_nanos();
        if last == 0 || elapsed == 0 {
            return None;
        }
        let bytes = u128::from(last.saturating_sub(first));
        Some((bytes * 1_000_000_000 / elapsed) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(5);

    /// Records `(seconds, total)` samples and checks the resulting rate.
    #[track_caller]
    fn check(samples: &[(f64, u64)], expected: Option<u64>) {
        let start = Instant::now();
        let mut meter = Meter::new(WINDOW);
        for &(seconds, total) in samples {
            meter.record(start + Duration::from_secs_f64(seconds), total);
        }
        assert_eq!(meter.rate(), expected, "{samples:?}");
    }

    #[test]
    fn a_rate_needs_two_samples() {
        check(&[], None);
        check(&[(0., 100)], None);
        check(&[(1., 100), (1., 200)], None);
    }

    #[test]
    fn nothing_counted_is_unknown() {
        check(&[(0., 0), (1., 0), (2., 0)], None);
    }

    #[test]
    fn a_steady_count_gives_its_rate() {
        check(&[(0., 0), (1., 1000)], Some(1000));
        check(&[(0., 0), (1., 1000), (2., 2000), (3., 3000)], Some(1000));
        check(&[(0., 0), (0.5, 500)], Some(1000));
        check(&[(0., 0), (3., 3000)], Some(1000));
    }

    #[test]
    fn the_rate_is_averaged_over_the_window() {
        check(&[(0., 0), (1., 5000), (2., 5000)], Some(2500));
        check(&[(0., 0), (1., 1000), (2., 1000), (3., 4000)], Some(1333));
    }

    /// 5000 bytes in the first second, then nothing until `until`.
    fn burst_then_stall(until: u64) -> Vec<(f64, u64)> {
        let stall = (1..=until).map(|second| (second as f64, 5000));
        [(0., 0)].into_iter().chain(stall).collect()
    }

    #[test]
    fn bytes_older_than_the_window_are_forgotten() {
        check(&burst_then_stall(5), Some(1000));
        check(&burst_then_stall(6), Some(0));
        check(&burst_then_stall(9), Some(0));
    }

    #[test]
    fn a_count_that_goes_back_is_no_progress() {
        check(&[(0., 1000), (1., 500)], Some(0));
    }
}
