use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Sliding-window rate limiter keyed by arbitrary string (e.g. email or IP).
///
/// Attempts within the configured `window` are counted per key.  Once the
/// count reaches `max_attempts` the key is rate-limited until old entries
/// fall outside the window.
#[derive(Debug)]
pub struct SlidingWindowRateLimiter {
    inner: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
    window: Duration,
    max_attempts: usize,
}

impl SlidingWindowRateLimiter {
    /// Creates a new limiter with the given window length and attempt cap.
    #[must_use]
    pub fn new(window: Duration, max_attempts: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            window,
            max_attempts,
        }
    }

    /// Returns `true` if this attempt is allowed and records it; `false` when
    /// the key has exhausted its quota within the current window.
    pub fn check_and_record(&self, key: &str) -> bool {
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        let bucket = state.entry(key.to_owned()).or_default();
        while bucket.front().is_some_and(|t| *t <= cutoff) {
            bucket.pop_front();
        }
        if bucket.len() >= self.max_attempts {
            return false;
        }
        bucket.push_back(now);
        true
    }
}

impl Clone for SlidingWindowRateLimiter {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            window: self.window,
            max_attempts: self.max_attempts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SlidingWindowRateLimiter;
    use std::time::Duration;

    #[test]
    fn rate_limiter_allows_attempts_under_limit() {
        let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(60), 3);
        assert!(limiter.check_and_record("user@example.com"));
        assert!(limiter.check_and_record("user@example.com"));
        assert!(limiter.check_and_record("user@example.com"));
    }

    #[test]
    fn rate_limiter_rejects_attempt_at_limit() {
        let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(60), 3);
        assert!(limiter.check_and_record("user@example.com"));
        assert!(limiter.check_and_record("user@example.com"));
        assert!(limiter.check_and_record("user@example.com"));
        assert!(!limiter.check_and_record("user@example.com"));
        assert!(!limiter.check_and_record("user@example.com"));
    }

    #[test]
    fn rate_limiter_tracks_keys_independently() {
        let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(60), 2);
        assert!(limiter.check_and_record("alice@example.com"));
        assert!(limiter.check_and_record("alice@example.com"));
        assert!(!limiter.check_and_record("alice@example.com"));
        assert!(limiter.check_and_record("bob@example.com"));
    }

    #[test]
    fn rate_limiter_shared_state_via_clone() {
        let limiter = SlidingWindowRateLimiter::new(Duration::from_secs(60), 2);
        let limiter2 = limiter.clone();
        assert!(limiter.check_and_record("user@example.com"));
        assert!(limiter.check_and_record("user@example.com"));
        assert!(!limiter2.check_and_record("user@example.com"));
    }
}
