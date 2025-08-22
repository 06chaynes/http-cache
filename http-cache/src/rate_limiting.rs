//! Rate limiting functionality for HTTP cache middleware
//!
//! This module provides traits and implementations for rate limiting HTTP requests
//! in a cache-aware manner, where rate limits are only applied on cache misses.

#[cfg(feature = "rate-limiting")]
use async_trait::async_trait;

#[cfg(feature = "rate-limiting")]
pub use governor::{
    clock::DefaultClock,
    state::{keyed::DefaultKeyedStateStore, InMemoryState},
    DefaultDirectRateLimiter, DefaultKeyedRateLimiter, Quota, RateLimiter,
};

/// A trait for rate limiting that can be implemented by different rate limiting strategies
#[cfg(feature = "rate-limiting")]
#[async_trait]
pub trait CacheAwareRateLimiter: Send + Sync + 'static {
    /// Wait until a request to the given key (typically a domain or URL) is allowed
    /// This method should block until the rate limit allows the request to proceed
    async fn until_key_ready(&self, key: &str);

    /// Check if a request to the given key would be allowed without blocking
    /// Returns true if the request can proceed immediately, false if it would be rate limited
    fn check_key(&self, key: &str) -> bool;
}

/// A domain-based rate limiter using governor that limits requests per domain
#[cfg(feature = "rate-limiting")]
#[derive(Debug)]
pub struct DomainRateLimiter {
    limiter: DefaultKeyedRateLimiter<String>,
}

#[cfg(feature = "rate-limiting")]
impl DomainRateLimiter {
    /// Create a new domain-based rate limiter with the given quota
    ///
    /// # Example
    /// ```rust,ignore
    /// use http_cache::rate_limiting::{DomainRateLimiter, Quota};
    /// use std::time::Duration;
    /// use std::num::NonZero;
    ///
    /// // Allow 10 requests per minute per domain
    /// let quota = Quota::per_minute(NonZero::new(10).unwrap());
    /// let limiter = DomainRateLimiter::new(quota);
    /// ```
    pub fn new(quota: Quota) -> Self {
        Self { limiter: DefaultKeyedRateLimiter::keyed(quota) }
    }
}

#[cfg(feature = "rate-limiting")]
#[async_trait]
impl CacheAwareRateLimiter for DomainRateLimiter {
    async fn until_key_ready(&self, key: &str) {
        self.limiter.until_key_ready(&key.to_string()).await;
    }

    fn check_key(&self, key: &str) -> bool {
        self.limiter.check_key(&key.to_string()).is_ok()
    }
}

/// A direct (non-keyed) rate limiter for simple use cases where all requests share the same limit
#[cfg(feature = "rate-limiting")]
#[derive(Debug)]
pub struct DirectRateLimiter {
    limiter: DefaultDirectRateLimiter,
}

#[cfg(feature = "rate-limiting")]
impl DirectRateLimiter {
    /// Create a direct (global) rate limiter that applies to all requests
    ///
    /// # Example
    /// ```rust,ignore
    /// use http_cache::rate_limiting::{DirectRateLimiter, Quota};
    /// use std::time::Duration;
    /// use std::num::NonZero;
    ///
    /// // Allow 10 requests per minute total
    /// let quota = Quota::per_minute(NonZero::new(10).unwrap());
    /// let limiter = DirectRateLimiter::direct(quota);
    /// ```
    pub fn direct(quota: Quota) -> DirectRateLimiter {
        DirectRateLimiter { limiter: DefaultDirectRateLimiter::direct(quota) }
    }
}

#[cfg(feature = "rate-limiting")]
#[async_trait]
impl CacheAwareRateLimiter for DirectRateLimiter {
    async fn until_key_ready(&self, _key: &str) {
        self.limiter.until_ready().await;
    }

    fn check_key(&self, _key: &str) -> bool {
        self.limiter.check().is_ok()
    }
}
