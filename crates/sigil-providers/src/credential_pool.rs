//! Credential pool — rotate multiple API keys for the same provider.
//!
//! Strategies: round_robin, least_used, random, fill_first.
//! Exhausted keys have cooldown periods (configurable).
//! Inspired by Hermes Agent's credential_pool.py.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Rotation strategy for selecting credentials.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RotationStrategy {
    /// Use keys in order, cycling through the list.
    RoundRobin,
    /// Use the key with the fewest total uses.
    LeastUsed,
    /// Pick a random available key.
    Random,
    /// Use the first key until it's exhausted, then move to the next.
    FillFirst,
}

impl Default for RotationStrategy {
    fn default() -> Self {
        Self::RoundRobin
    }
}

/// A single credential with usage tracking and cooldown state.
#[derive(Debug)]
struct Credential {
    key: String,
    use_count: AtomicU64,
    exhausted_at: Option<Instant>,
    cooldown: Duration,
}

impl Credential {
    fn new(key: String, cooldown: Duration) -> Self {
        Self {
            key,
            use_count: AtomicU64::new(0),
            exhausted_at: None,
            cooldown,
        }
    }

    fn is_available(&self) -> bool {
        match self.exhausted_at {
            None => true,
            Some(at) => at.elapsed() >= self.cooldown,
        }
    }

    fn mark_used(&self) {
        self.use_count.fetch_add(1, Ordering::Relaxed);
    }

    fn mark_exhausted(&mut self) {
        self.exhausted_at = Some(Instant::now());
    }

    fn uses(&self) -> u64 {
        self.use_count.load(Ordering::Relaxed)
    }
}

/// Pool of API credentials with rotation and cooldown.
pub struct CredentialPool {
    credentials: Vec<Credential>,
    strategy: RotationStrategy,
    next_index: usize,
    /// Cooldown for rate-limited keys (default: 1 hour).
    rate_limit_cooldown: Duration,
    /// Cooldown for billing/auth errors (default: 24 hours).
    auth_error_cooldown: Duration,
}

impl CredentialPool {
    /// Create a pool from a list of API keys.
    pub fn new(keys: Vec<String>, strategy: RotationStrategy) -> Self {
        let rate_limit_cooldown = Duration::from_secs(3600); // 1 hour
        let credentials = keys
            .into_iter()
            .map(|k| Credential::new(k, rate_limit_cooldown))
            .collect();

        Self {
            credentials,
            strategy,
            next_index: 0,
            rate_limit_cooldown,
            auth_error_cooldown: Duration::from_secs(86400), // 24 hours
        }
    }

    /// Get the next available credential key. Returns None if all are exhausted.
    pub fn next_key(&mut self) -> Option<&str> {
        if self.credentials.is_empty() {
            return None;
        }

        let available: Vec<usize> = self
            .credentials
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_available())
            .map(|(i, _)| i)
            .collect();

        if available.is_empty() {
            warn!("all credentials exhausted — no available keys");
            return None;
        }

        let idx = match self.strategy {
            RotationStrategy::RoundRobin => {
                let start = self.next_index % self.credentials.len();
                let idx = (start..self.credentials.len())
                    .chain(0..start)
                    .find(|i| available.contains(i))
                    .unwrap_or(available[0]);
                self.next_index = idx + 1;
                idx
            }
            RotationStrategy::LeastUsed => *available
                .iter()
                .min_by_key(|&&i| self.credentials[i].uses())
                .unwrap(),
            RotationStrategy::Random => {
                use std::time::SystemTime;
                let seed = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as usize;
                available[seed % available.len()]
            }
            RotationStrategy::FillFirst => available[0],
        };

        self.credentials[idx].mark_used();
        debug!(
            strategy = ?self.strategy,
            key_index = idx,
            uses = self.credentials[idx].uses(),
            "credential selected"
        );
        Some(&self.credentials[idx].key)
    }

    /// Mark a key as rate-limited (429). It enters cooldown.
    pub fn mark_rate_limited(&mut self, key: &str) {
        if let Some(cred) = self.credentials.iter_mut().find(|c| c.key == key) {
            cred.cooldown = self.rate_limit_cooldown;
            cred.mark_exhausted();
            warn!(
                cooldown_secs = self.rate_limit_cooldown.as_secs(),
                "credential rate-limited"
            );
        }
    }

    /// Mark a key as having an auth/billing error. Longer cooldown.
    pub fn mark_auth_error(&mut self, key: &str) {
        if let Some(cred) = self.credentials.iter_mut().find(|c| c.key == key) {
            cred.cooldown = self.auth_error_cooldown;
            cred.mark_exhausted();
            warn!(
                cooldown_secs = self.auth_error_cooldown.as_secs(),
                "credential auth error — long cooldown"
            );
        }
    }

    /// Number of currently available credentials.
    pub fn available_count(&self) -> usize {
        self.credentials.iter().filter(|c| c.is_available()).count()
    }

    /// Total number of credentials in the pool.
    pub fn total_count(&self) -> usize {
        self.credentials.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_robin() {
        let mut pool = CredentialPool::new(
            vec!["key1".into(), "key2".into(), "key3".into()],
            RotationStrategy::RoundRobin,
        );
        assert_eq!(pool.next_key(), Some("key1"));
        assert_eq!(pool.next_key(), Some("key2"));
        assert_eq!(pool.next_key(), Some("key3"));
        assert_eq!(pool.next_key(), Some("key1")); // Wraps around
    }

    #[test]
    fn test_fill_first() {
        let mut pool = CredentialPool::new(
            vec!["key1".into(), "key2".into()],
            RotationStrategy::FillFirst,
        );
        assert_eq!(pool.next_key(), Some("key1"));
        assert_eq!(pool.next_key(), Some("key1"));
        assert_eq!(pool.next_key(), Some("key1"));
    }

    #[test]
    fn test_least_used() {
        let mut pool = CredentialPool::new(
            vec!["key1".into(), "key2".into()],
            RotationStrategy::LeastUsed,
        );
        assert_eq!(pool.next_key(), Some("key1")); // Both at 0, picks first
        assert_eq!(pool.next_key(), Some("key2")); // key1 at 1, key2 at 0
        assert_eq!(pool.next_key(), Some("key1")); // Both at 1, picks first
    }

    #[test]
    fn test_rate_limit_cooldown() {
        let mut pool = CredentialPool::new(
            vec!["key1".into(), "key2".into()],
            RotationStrategy::RoundRobin,
        );
        pool.mark_rate_limited("key1");
        // key1 is now in cooldown, should skip to key2
        assert_eq!(pool.next_key(), Some("key2"));
        assert_eq!(pool.available_count(), 1);
    }

    #[test]
    fn test_all_exhausted() {
        let mut pool = CredentialPool::new(
            vec!["key1".into()],
            RotationStrategy::RoundRobin,
        );
        pool.mark_rate_limited("key1");
        assert_eq!(pool.next_key(), None);
        assert_eq!(pool.available_count(), 0);
    }

    #[test]
    fn test_empty_pool() {
        let mut pool = CredentialPool::new(vec![], RotationStrategy::RoundRobin);
        assert_eq!(pool.next_key(), None);
        assert_eq!(pool.total_count(), 0);
    }
}
