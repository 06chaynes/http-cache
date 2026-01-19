//! HTTP caching manager implementation using QuickCache.
//!
//! This crate provides a [`CacheManager`] implementation using the
//! [QuickCache](https://github.com/arthurprs/quick-cache) in-memory cache.
//! QuickCache is an in-memory cache that can be used for applications that
//! need cache access with predictable memory usage.
//!
//! ## Basic Usage
//!
//! ```rust
//! use http_cache_quickcache::QuickManager;
//! use quick_cache::sync::Cache;
//!
//! // Create a cache with a maximum of 1000 entries
//! let cache = Cache::new(1000);
//! let manager = QuickManager::new(cache);
//!
//! // Use with any HTTP cache implementation that accepts a CacheManager
//! ```
//!
//! ## Integration with HTTP Cache Middleware
//!
//! ### With Tower Services
//!
//! ```no_run
//! use tower::{Service, ServiceExt};
//! use http::{Request, Response, StatusCode};
//! use http_body_util::Full;
//! use bytes::Bytes;
//! use http_cache_quickcache::QuickManager;
//! use std::convert::Infallible;
//!
//! // Example Tower service that uses QuickManager for caching
//! #[derive(Clone)]
//! struct CachingService {
//!     cache_manager: QuickManager,
//! }
//!
//! impl Service<Request<Full<Bytes>>> for CachingService {
//!     type Response = Response<Full<Bytes>>;
//!     type Error = Box<dyn std::error::Error + Send + Sync>;
//!     type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;
//!
//!     fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
//!         std::task::Poll::Ready(Ok(()))
//!     }
//!
//!     fn call(&mut self, req: Request<Full<Bytes>>) -> Self::Future {
//!         let manager = self.cache_manager.clone();
//!         Box::pin(async move {
//!             // Cache logic using the manager would go here
//!             let response = Response::builder()
//!                 .status(StatusCode::OK)
//!                 .body(Full::new(Bytes::from("Hello from cached service!")))?;
//!             Ok(response)
//!         })
//!     }
//! }
//! ```
//!
//! ### With Hyper
//!
//! ```no_run
//! use hyper::{Request, Response, StatusCode, body::Incoming};
//! use http_body_util::Full;
//! use bytes::Bytes;
//! use http_cache_quickcache::QuickManager;
//! use std::convert::Infallible;
//!
//! async fn handle_request(
//!     _req: Request<Incoming>,
//!     cache_manager: QuickManager,
//! ) -> Result<Response<Full<Bytes>>, Infallible> {
//!     // Use cache_manager here for caching responses
//!     Ok(Response::builder()
//!         .status(StatusCode::OK)
//!         .header("cache-control", "max-age=3600")
//!         .body(Full::new(Bytes::from("Hello from Hyper with caching!")))
//!         .unwrap())
//! }
//! ```
//!
//! ## Usage Characteristics
//!
//! QuickCache is designed for scenarios where:
//! - You need predictable memory usage
//! - In-memory storage is acceptable
//! - You want to avoid complex configuration
//! - Memory-based caching fits your use case
//!
//! For applications that need persistent caching across restarts, consider using
//! [`CACacheManager`](https://docs.rs/http-cache/latest/http_cache/struct.CACacheManager.html)
//! instead, which provides disk-based storage.

use http_cache::{CacheManager, HttpResponse, Result};

use std::{fmt, sync::Arc};

use http_cache_semantics::CachePolicy;
use quick_cache::sync::Cache;
use serde::{Deserialize, Serialize};

/// HTTP cache manager implementation using QuickCache.
///
/// This manager provides in-memory caching using the QuickCache library and implements
/// the [`CacheManager`] trait for HTTP caching support.
///
/// ## Examples
///
/// ### Basic Usage
///
/// ```rust
/// use http_cache_quickcache::QuickManager;
/// use quick_cache::sync::Cache;
///
/// // Create a cache with 1000 entry limit
/// let cache = Cache::new(1000);
/// let manager = QuickManager::new(cache);
/// ```
///
/// ## Default Configuration
///
/// The default configuration creates a cache with 42 entries:
///
/// ```rust
/// use http_cache_quickcache::QuickManager;
///
/// let manager = QuickManager::default();
/// ```
#[derive(Clone)]
pub struct QuickManager {
    /// The underlying QuickCache instance.
    ///
    /// This is wrapped in an `Arc` to allow sharing across threads while
    /// maintaining the `Clone` implementation for the manager.
    cache: Arc<Cache<String, Arc<Vec<u8>>>>,
}

impl fmt::Debug for QuickManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("QuickManager")
            .field("cache", &"Cache<String, Arc<Vec<u8>>>")
            .finish_non_exhaustive()
    }
}

impl Default for QuickManager {
    /// Creates a new QuickManager with a default cache size of 42 entries.
    ///
    /// For production use, consider using [`QuickManager::new`] with a
    /// cache size appropriate for your use case.
    fn default() -> Self {
        Self::new(Cache::new(42))
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}

impl QuickManager {
    /// Creates a new QuickManager from a pre-configured QuickCache.
    ///
    /// This allows you to customize the cache configuration, such as setting
    /// the maximum number of entries.
    ///
    /// # Arguments
    ///
    /// * `cache` - A configured QuickCache instance
    ///
    /// # Examples
    ///
    /// ```rust
    /// use http_cache_quickcache::QuickManager;
    /// use quick_cache::sync::Cache;
    ///
    /// // Create a cache with 10,000 entry limit
    /// let cache = Cache::new(10_000);
    /// let manager = QuickManager::new(cache);
    /// ```
    ///
    /// ## Cache Size Considerations
    ///
    /// Choose your cache size based on:
    /// - Available memory
    /// - Expected number of unique cacheable requests
    /// - Average response size
    /// - Cache hit rate requirements
    ///
    /// ```rust
    /// use http_cache_quickcache::QuickManager;
    /// use quick_cache::sync::Cache;
    ///
    /// // For an application with many unique endpoints
    /// let large_cache = QuickManager::new(Cache::new(50_000));
    ///
    /// // For an application with few cacheable responses
    /// let small_cache = QuickManager::new(Cache::new(100));
    /// ```
    pub fn new(cache: Cache<String, Arc<Vec<u8>>>) -> Self {
        Self { cache: Arc::new(cache) }
    }
}

#[async_trait::async_trait]
impl CacheManager for QuickManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        let store: Store = match self.cache.get(cache_key) {
            Some(d) => postcard::from_bytes(&d)?,
            None => return Ok(None),
        };
        Ok(Some((store.response, store.policy)))
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        let data = Store { response: response.clone(), policy };
        let bytes = postcard::to_allocvec(&data)?;
        self.cache.insert(cache_key, Arc::new(bytes));
        Ok(response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        self.cache.remove(cache_key);
        Ok(())
    }
}

#[cfg(test)]
mod test;
