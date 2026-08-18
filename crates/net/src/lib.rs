use chrono::{DateTime, Utc};
use http::{HeaderMap, header::RETRY_AFTER};
use rand::RngExt;
use std::time::Duration;
use url::Url;

mod connectivity;

pub use connectivity::*;

pub const JITTER_MAX: Duration = Duration::from_millis(500);

pub trait HeaderMapExt {
    fn retry_after(&self, now: DateTime<Utc>) -> Option<Duration>;
}

impl HeaderMapExt for HeaderMap {
    fn retry_after(&self, now: DateTime<Utc>) -> Option<Duration> {
        let value = self.get(RETRY_AFTER)?.to_str().ok()?.trim();

        if let Ok(seconds) = value.parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }

        let deadline = DateTime::parse_from_rfc2822(value).ok()?;
        (deadline.to_utc() - now).to_std().ok()
    }
}

/// How long to wait before trying again, doubling with every attempt.
pub fn backoff(attempts: u32) -> Duration {
    Duration::from_secs(2u64.pow(attempts.saturating_sub(1))) + jitter()
}

/// A short random delay, so callers that were held back by the same thing do
/// not all come back at once.
pub fn jitter() -> Duration {
    rand::rng().random_range(Duration::ZERO..JITTER_MAX)
}

pub fn host_of(url: &str) -> Option<String> {
    Url::parse(url).ok()?.host_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;

    /// Tests are measured against this date.
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc2822("Wed, 21 Oct 2099 03:46:00 GMT")
            .expect("the test date should parse")
            .to_utc()
    }

    #[track_caller]
    fn check_retry_after(header: Option<&'static str>, expected: Option<Duration>) {
        let mut headers = HeaderMap::new();
        if let Some(value) = header {
            headers.insert(RETRY_AFTER, HeaderValue::from_static(value));
        }

        assert_eq!(headers.retry_after(now()), expected, "header {header:?}");
    }

    #[test]
    fn retry_after_reads_a_delay() {
        for (header, expected) in [
            ("120", Duration::from_secs(120)),
            ("0", Duration::ZERO),
            ("  120  ", Duration::from_secs(120)),
            ("Wed, 21 Oct 2099 03:48:00 GMT", Duration::from_secs(120)),
        ] {
            check_retry_after(Some(header), Some(expected));
        }
    }

    #[test]
    fn retry_after_ignores_what_it_cannot_use() {
        for header in [
            None,
            Some("soon"),
            Some("-5"),
            // A date that has already passed.
            Some("Wed, 30 Jun 2015 16:13:00 GMT"),
        ] {
            check_retry_after(header, None);
        }
    }

    #[track_caller]
    fn check_backoff(attempts: u32, base: Duration) {
        let delay = backoff(attempts);

        assert!(
            (base..base + JITTER_MAX).contains(&delay),
            "backoff for attempt {attempts} was {delay:?}, expected about {base:?}"
        );
    }

    #[test]
    fn backoff_doubles_and_carries_jitter() {
        for (attempts, base) in [(0, 1), (1, 1), (2, 2), (3, 4), (4, 8)] {
            check_backoff(attempts, Duration::from_secs(base));
        }
    }

    #[track_caller]
    fn check_host(url: &str, expected: Option<&str>) {
        assert_eq!(host_of(url).as_deref(), expected, "host of {url}");
    }

    #[test]
    fn hosts_come_from_the_url() {
        check_host(
            "https://images.example.com/cover.png",
            Some("images.example.com"),
        );
        check_host("http://127.0.0.1:8080/cover.png", Some("127.0.0.1"));
        check_host("cover.png", None);
        check_host("file:///covers/cover.png", None);
    }
}
