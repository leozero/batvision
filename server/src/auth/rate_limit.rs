//! Brute-force protection: after a few failures, a key (IP or username) is locked
//! for a delay that doubles with each further failure.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
pub struct Policy {
    /// Failures allowed before the first lock.
    pub free_failures: u32,
    pub base_lock: Duration,
    pub max_lock: Duration,
    /// A key with no failure for this long is forgotten.
    pub forget_after: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            free_failures: 5,
            base_lock: Duration::from_secs(30),
            max_lock: Duration::from_secs(15 * 60),
            forget_after: Duration::from_secs(60 * 60),
        }
    }
}

#[derive(Debug)]
struct Entry {
    failures: u32,
    last_failure: Instant,
    locked_until: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct RateLimiter {
    policy: Policy,
    entries: Mutex<HashMap<String, Entry>>,
}

impl RateLimiter {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// `Err(retry_after)` if any of `keys` is currently locked.
    pub fn check(&self, keys: &[&str], now: Instant) -> Result<(), Duration> {
        let entries = self.entries.lock().unwrap();
        let wait = keys
            .iter()
            .filter_map(|key| entries.get(*key)?.locked_until)
            .filter(|until| *until > now)
            .map(|until| until - now)
            .max();
        match wait {
            Some(wait) => Err(wait),
            None => Ok(()),
        }
    }

    pub fn record_failure(&self, keys: &[&str], now: Instant) {
        let mut entries = self.entries.lock().unwrap();
        let policy = self.policy;
        entries.retain(|_, e| now.duration_since(e.last_failure) < policy.forget_after);
        for key in keys {
            let entry = entries.entry((*key).to_owned()).or_insert(Entry {
                failures: 0,
                last_failure: now,
                locked_until: None,
            });
            entry.failures += 1;
            entry.last_failure = now;
            if entry.failures >= policy.free_failures {
                let exponent = (entry.failures - policy.free_failures).min(16);
                let lock = policy
                    .base_lock
                    .saturating_mul(1 << exponent)
                    .min(policy.max_lock);
                entry.locked_until = Some(now + lock);
            }
        }
    }

    pub fn record_success(&self, keys: &[&str]) {
        let mut entries = self.entries.lock().unwrap();
        for key in keys {
            entries.remove(*key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter() -> RateLimiter {
        RateLimiter::new(Policy {
            free_failures: 3,
            base_lock: Duration::from_secs(10),
            max_lock: Duration::from_secs(60),
            forget_after: Duration::from_secs(3600),
        })
    }

    #[test]
    fn allows_free_failures_then_locks() {
        let rl = limiter();
        let t0 = Instant::now();
        for _ in 0..2 {
            rl.record_failure(&["user:bob"], t0);
            assert!(rl.check(&["user:bob"], t0).is_ok());
        }
        rl.record_failure(&["user:bob"], t0);
        assert_eq!(rl.check(&["user:bob"], t0), Err(Duration::from_secs(10)));
        assert!(
            rl.check(&["user:bob"], t0 + Duration::from_secs(11))
                .is_ok()
        );
    }

    #[test]
    fn lock_doubles_and_is_capped() {
        let rl = limiter();
        let t0 = Instant::now();
        for _ in 0..4 {
            rl.record_failure(&["k"], t0);
        }
        assert_eq!(rl.check(&["k"], t0), Err(Duration::from_secs(20)));
        for _ in 0..10 {
            rl.record_failure(&["k"], t0);
        }
        assert_eq!(rl.check(&["k"], t0), Err(Duration::from_secs(60)));
    }

    #[test]
    fn success_clears_and_keys_are_independent() {
        let rl = limiter();
        let t0 = Instant::now();
        for _ in 0..3 {
            rl.record_failure(&["ip:1", "user:a"], t0);
        }
        assert!(rl.check(&["user:b"], t0).is_ok());
        assert!(rl.check(&["ip:1", "user:b"], t0).is_err());
        rl.record_success(&["ip:1", "user:a"]);
        assert!(rl.check(&["ip:1", "user:a"], t0).is_ok());
    }
}
