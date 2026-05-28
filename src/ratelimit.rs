use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;

/// Maximum number of keys before a forced full sweep is triggered.
const MAX_KEYS: usize = 10_000;

/// Best-effort client IP extraction from common reverse-proxy headers,
/// falling back to a shared "unknown" bucket.
///
/// Behind Tailscale Funnel / nginx the `X-Forwarded-For` header is set;
/// for direct local connections the fallback bucket is fine because it
/// just means *all* unknown sources share one rate-limit budget (still
/// better than no per-IP limiting at all). Used by both the OAuth
/// registration limiter (LIF-64) and the login limiter (LIF-75).
pub fn client_ip(headers: &HeaderMap) -> String {
    if let Some(v) = headers.get("x-forwarded-for")
        && let Ok(s) = v.to_str()
    {
        // First IP in the comma-separated list is the original client.
        if let Some(first) = s.split(',').next() {
            let trimmed = first.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    if let Some(v) = headers.get("x-real-ip")
        && let Ok(s) = v.to_str()
    {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "unknown".to_string()
}

/// Simple in-memory rate limiter.
/// Tracks attempts per key (e.g. username or IP) within a sliding window.
/// Expired keys are evicted periodically to prevent unbounded memory growth.
#[derive(Debug)]
pub struct RateLimiter {
    /// (key -> list of attempt timestamps)
    attempts: Mutex<HashMap<String, Vec<Instant>>>,
    /// Maximum attempts allowed within the window.
    max_attempts: usize,
    /// Window duration.
    window: Duration,
}

impl RateLimiter {
    pub fn new(max_attempts: usize, window: Duration) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            max_attempts,
            window,
        }
    }

    /// Remove all keys whose attempt lists are empty or fully expired.
    fn sweep(map: &mut HashMap<String, Vec<Instant>>, window: Duration) {
        let now = Instant::now();
        map.retain(|_, entries| {
            entries.retain(|t| now.duration_since(*t) < window);
            !entries.is_empty()
        });
    }

    /// Record an attempt for the given key.
    /// Returns `true` if the attempt is allowed, `false` if rate-limited.
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut map = self.attempts.lock().unwrap_or_else(|e| e.into_inner());

        // Evict expired keys if map is getting large
        if map.len() > MAX_KEYS {
            Self::sweep(&mut map, self.window);
        }

        let entry = map.entry(key.to_string()).or_default();

        // Prune expired entries for this key
        entry.retain(|t| now.duration_since(*t) < self.window);

        if entry.len() >= self.max_attempts {
            return false;
        }

        entry.push(now);
        true
    }

    /// Test whether an attempt *would* be allowed, WITHOUT recording it.
    ///
    /// This exists so the login path can separate the "are we over the
    /// limit?" question from the "record a failure" action. Previously
    /// login called `check()` (which records on pass) AND
    /// `record_failure()`, so each failed login consumed two slots and
    /// halved the effective limit (LIF-75). Login now `peek()`s, then
    /// records exactly one failure via `record_failure()` on auth failure.
    pub fn peek(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut map = self.attempts.lock().unwrap_or_else(|e| e.into_inner());
        match map.get_mut(key) {
            Some(entry) => {
                entry.retain(|t| now.duration_since(*t) < self.window);
                entry.len() < self.max_attempts
            }
            None => true,
        }
    }

    /// Record a failed attempt without checking first (for tracking after auth failure).
    pub fn record_failure(&self, key: &str) {
        let now = Instant::now();
        let mut map = self.attempts.lock().unwrap_or_else(|e| e.into_inner());

        if map.len() > MAX_KEYS {
            Self::sweep(&mut map, self.window);
        }

        let entry = map.entry(key.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < self.window);
        entry.push(now);
    }

    /// How many seconds until the oldest attempt in the window expires.
    pub fn retry_after(&self, key: &str) -> u64 {
        let now = Instant::now();
        let map = self.attempts.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(key) {
            Some(entries) if !entries.is_empty() => {
                let oldest = entries[0];
                let elapsed = now.duration_since(oldest);
                if elapsed < self.window {
                    (self.window - elapsed).as_secs() + 1
                } else {
                    0
                }
            }
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_under_limit() {
        let rl = RateLimiter::new(3, Duration::from_secs(60));
        assert!(rl.check("user1"));
        assert!(rl.check("user1"));
        assert!(rl.check("user1"));
    }

    #[test]
    fn blocks_over_limit() {
        let rl = RateLimiter::new(2, Duration::from_secs(60));
        assert!(rl.check("user1"));
        assert!(rl.check("user1"));
        assert!(!rl.check("user1")); // blocked
    }

    #[test]
    fn different_keys_independent() {
        let rl = RateLimiter::new(1, Duration::from_secs(60));
        assert!(rl.check("user1"));
        assert!(rl.check("user2")); // different key, still allowed
        assert!(!rl.check("user1")); // same key, blocked
    }

    #[test]
    fn retry_after_nonzero_when_limited() {
        let rl = RateLimiter::new(1, Duration::from_secs(60));
        rl.check("user1");
        assert!(rl.retry_after("user1") > 0);
    }

    #[test]
    fn sweep_removes_expired_keys() {
        let mut map: HashMap<String, Vec<Instant>> = HashMap::new();
        let window = Duration::from_millis(1);

        // Insert an entry that will be expired by the time we sweep
        map.insert("old".into(), vec![Instant::now()]);

        // Wait for it to expire
        std::thread::sleep(Duration::from_millis(5));

        RateLimiter::sweep(&mut map, window);
        assert!(map.is_empty(), "expired keys should be evicted");
    }

    // ── LIF-75: peek() is non-recording ──────────────────────

    #[test]
    fn peek_does_not_record() {
        let rl = RateLimiter::new(1, Duration::from_secs(60));
        // Peeking repeatedly never consumes the budget.
        assert!(rl.peek("user1"));
        assert!(rl.peek("user1"));
        assert!(rl.peek("user1"));
        // A single recorded failure is enough to hit the limit of 1.
        rl.record_failure("user1");
        assert!(
            !rl.peek("user1"),
            "should be at limit after exactly one recorded failure"
        );
    }

    #[test]
    fn peek_reflects_recorded_failures_one_to_one() {
        // Proves the double-counting fix: N recorded failures == N slots,
        // not 2N. With max 5, the 5th failure is still allowed, 6th blocked.
        let rl = RateLimiter::new(5, Duration::from_secs(60));
        for i in 0..5 {
            assert!(rl.peek("u"), "attempt {i} should be allowed");
            rl.record_failure("u");
        }
        assert!(!rl.peek("u"), "6th attempt should be blocked");
    }

    // ── LIF-75: client_ip header extraction ──────────────────

    #[test]
    fn client_ip_prefers_x_forwarded_for_first_hop() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.7, 10.0.0.1".parse().unwrap());
        assert_eq!(client_ip(&h), "203.0.113.7");
    }

    #[test]
    fn client_ip_falls_back_to_x_real_ip() {
        let mut h = HeaderMap::new();
        h.insert("x-real-ip", "198.51.100.4".parse().unwrap());
        assert_eq!(client_ip(&h), "198.51.100.4");
    }

    #[test]
    fn client_ip_unknown_when_no_proxy_headers() {
        let h = HeaderMap::new();
        assert_eq!(client_ip(&h), "unknown");
    }
}
