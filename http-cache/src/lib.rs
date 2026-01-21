#![forbid(unsafe_code, future_incompatible)]
#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    nonstandard_style,
    unused_qualifications,
    unused_import_braces,
    unused_extern_crates,
    trivial_casts,
    trivial_numeric_casts
)]
#![allow(clippy::doc_lazy_continuation)]
#![cfg_attr(docsrs, feature(doc_cfg))]
//! A caching middleware that follows HTTP caching rules, thanks to
//! [`http-cache-semantics`](https://github.com/kornelski/rusty-http-cache-semantics).
//! By default, it uses [`cacache`](https://github.com/zkat/cacache-rs) as the backend cache manager.
//!
//! This crate provides the core HTTP caching functionality that can be used to build
//! caching middleware for various HTTP clients and server frameworks. It implements
//! RFC 7234 HTTP caching semantics, supporting features like:
//!
//! - Automatic cache invalidation for unsafe HTTP methods (PUT, POST, DELETE, PATCH)
//! - Respect for HTTP cache-control headers
//! - Conditional requests (ETag, Last-Modified)
//! - Multiple cache storage backends
//! - Streaming response support
//!
//! ## Basic Usage
//!
//! The core types for building HTTP caches:
//!
//! ```rust
//! # #[cfg(feature = "manager-cacache")]
//! # fn main() {
//! use http_cache::{CACacheManager, HttpCache, CacheMode, HttpCacheOptions};
//!
//! // Create a cache manager with disk storage
//! let manager = CACacheManager::new("./cache".into(), true);
//!
//! // Create an HTTP cache with default behavior
//! let cache = HttpCache {
//!     mode: CacheMode::Default,
//!     manager,
//!     options: HttpCacheOptions::default(),
//! };
//! # }
//! # #[cfg(not(feature = "manager-cacache"))]
//! # fn main() {}
//! ```
//!
//! ## Cache Modes
//!
//! Different cache modes provide different behaviors:
//!
//! ```rust
//! # #[cfg(feature = "manager-cacache")]
//! # fn main() {
//! use http_cache::{CacheMode, HttpCache, CACacheManager, HttpCacheOptions};
//!
//! let manager = CACacheManager::new("./cache".into(), true);
//!
//! // Default mode: follows HTTP caching rules
//! let default_cache = HttpCache {
//!     mode: CacheMode::Default,
//!     manager: manager.clone(),
//!     options: HttpCacheOptions::default(),
//! };
//!
//! // NoStore mode: never caches responses
//! let no_store_cache = HttpCache {
//!     mode: CacheMode::NoStore,
//!     manager: manager.clone(),
//!     options: HttpCacheOptions::default(),
//! };
//!
//! // ForceCache mode: caches responses even if headers suggest otherwise
//! let force_cache = HttpCache {
//!     mode: CacheMode::ForceCache,
//!     manager,
//!     options: HttpCacheOptions::default(),
//! };
//! # }
//! # #[cfg(not(feature = "manager-cacache"))]
//! # fn main() {}
//! ```
//!
//! ## Custom Cache Keys
//!
//! You can customize how cache keys are generated:
//!
//! ```rust
//! # #[cfg(feature = "manager-cacache")]
//! # fn main() {
//! use http_cache::{HttpCacheOptions, CACacheManager, HttpCache, CacheMode};
//! use std::sync::Arc;
//! use http::request::Parts;
//!
//! let manager = CACacheManager::new("./cache".into(), true);
//!
//! let options = HttpCacheOptions {
//!     cache_key: Some(Arc::new(|req: &Parts| {
//!         // Custom cache key that includes query parameters
//!         format!("{}:{}", req.method, req.uri)
//!     })),
//!     ..Default::default()
//! };
//!
//! let cache = HttpCache {
//!     mode: CacheMode::Default,
//!     manager,
//!     options,
//! };
//! # }
//! # #[cfg(not(feature = "manager-cacache"))]
//! # fn main() {}
//! ```
//!
//! ## Maximum TTL Control
//!
//! Set a maximum time-to-live for cached responses, particularly useful with `CacheMode::IgnoreRules`:
//!
//! ```rust
//! # #[cfg(feature = "manager-cacache")]
//! # fn main() {
//! use http_cache::{HttpCacheOptions, CACacheManager, HttpCache, CacheMode};
//! use std::time::Duration;
//!
//! let manager = CACacheManager::new("./cache".into(), true);
//!
//! // Limit cache duration to 5 minutes regardless of server headers
//! let options = HttpCacheOptions {
//!     max_ttl: Some(Duration::from_secs(300)), // 5 minutes
//!     ..Default::default()
//! };
//!
//! let cache = HttpCache {
//!     mode: CacheMode::IgnoreRules, // Ignore server cache-control headers
//!     manager,
//!     options,
//! };
//! # }
//! # #[cfg(not(feature = "manager-cacache"))]
//! # fn main() {}
//! ```
//!
//! ## Response-Based Cache Mode Override
//!
//! Override cache behavior based on the response you receive. This is useful for scenarios like
//! forcing cache for successful responses even when headers say not to cache, or never caching
//! error responses like rate limits:
//!
//! ```rust
//! # #[cfg(feature = "manager-cacache")]
//! # fn main() {
//! use http_cache::{HttpCacheOptions, CACacheManager, HttpCache, CacheMode};
//! use std::sync::Arc;
//!
//! let manager = CACacheManager::new("./cache".into(), true);
//!
//! let options = HttpCacheOptions {
//!     response_cache_mode_fn: Some(Arc::new(|_request_parts, response| {
//!         match response.status {
//!             // Force cache successful responses even if headers say not to cache
//!             200..=299 => Some(CacheMode::ForceCache),
//!             // Never cache rate-limited responses
//!             429 => Some(CacheMode::NoStore),
//!             // Use default behavior for everything else
//!             _ => None,
//!         }
//!     })),
//!     ..Default::default()
//! };
//!
//! let cache = HttpCache {
//!     mode: CacheMode::Default,
//!     manager,
//!     options,
//! };
//! # }
//! # #[cfg(not(feature = "manager-cacache"))]
//! # fn main() {}
//! ```
//!
//! ## Content-Type Based Caching
//!
//! You can implement selective caching based on response content types using `response_cache_mode_fn`.
//! This is useful when you only want to cache certain types of content:
//!
//! ```rust
//! # #[cfg(feature = "manager-cacache")]
//! # fn main() {
//! use http_cache::{HttpCacheOptions, CACacheManager, HttpCache, CacheMode};
//! use std::sync::Arc;
//!
//! let manager = CACacheManager::new("./cache".into(), true);
//!
//! let options = HttpCacheOptions {
//!     response_cache_mode_fn: Some(Arc::new(|_request_parts, response| {
//!         // Check the Content-Type header to decide caching behavior
//!         if let Some(content_type) = response.headers.get("content-type") {
//!             match content_type.as_str() {
//!                 // Cache JSON APIs aggressively
//!                 ct if ct.starts_with("application/json") => Some(CacheMode::ForceCache),
//!                 // Cache images with default rules
//!                 ct if ct.starts_with("image/") => Some(CacheMode::Default),
//!                 // Cache static assets
//!                 ct if ct.starts_with("text/css") => Some(CacheMode::ForceCache),
//!                 ct if ct.starts_with("application/javascript") => Some(CacheMode::ForceCache),
//!                 // Don't cache HTML pages (dynamic content)
//!                 ct if ct.starts_with("text/html") => Some(CacheMode::NoStore),
//!                 // Don't cache unknown content types
//!                 _ => Some(CacheMode::NoStore),
//!             }
//!         } else {
//!             // No Content-Type header - don't cache
//!             Some(CacheMode::NoStore)
//!         }
//!     })),
//!     ..Default::default()
//! };
//!
//! let cache = HttpCache {
//!     mode: CacheMode::Default, // This gets overridden by response_cache_mode_fn
//!     manager,
//!     options,
//! };
//! # }
//! # #[cfg(not(feature = "manager-cacache"))]
//! # fn main() {}
//! ```
//!
//! ## Streaming Support
//!
//! For handling large responses without full buffering, use the `StreamingManager`:
//!
//! ```rust
//! # #[cfg(feature = "streaming")]
//! # {
//! use http_cache::{StreamingBody, HttpStreamingCache, StreamingManager};
//! use bytes::Bytes;
//! use std::path::PathBuf;
//! use http_body::Body;
//! use http_body_util::Full;
//!
//! // Create a file-based streaming cache manager
//! let manager = StreamingManager::new(PathBuf::from("./streaming-cache"));
//!
//! // StreamingBody can handle both buffered and streaming scenarios
//! let body: StreamingBody<Full<Bytes>> = StreamingBody::buffered(Bytes::from("cached content"));
//! println!("Body size: {:?}", body.size_hint());
//! # }
//! ```
//!
//! **Note**: Streaming support requires the `StreamingManager` with the `streaming` feature.
//! Other cache managers (CACacheManager, MokaManager, QuickManager) do not support streaming
//! and will buffer response bodies in memory.
//!
//! ## Features
//!
//! The following features are available. By default `manager-cacache` is enabled.
//!
//! - `manager-cacache` (default): enable [cacache](https://github.com/zkat/cacache-rs),
//! a disk cache, backend manager. Uses tokio runtime.
//! - `manager-moka` (disabled): enable [moka](https://github.com/moka-rs/moka),
//! an in-memory cache, backend manager.
//! - `manager-foyer` (disabled): enable [foyer](https://github.com/foyer-rs/foyer),
//! a hybrid in-memory + disk cache, backend manager. Uses tokio runtime.
//! - `http-headers-compat` (disabled): enable backwards compatibility for deserializing cached
//! responses from older versions that used single-value headers. Enable this if you need to read
//! cache entries created by older versions of http-cache.
//! - `streaming` (disabled): enable the `StreamingManager` for streaming cache support.
//! - `streaming-tokio` (disabled): enable streaming with tokio runtime support.
//! - `streaming-smol` (disabled): enable streaming with smol runtime support.
//! - `with-http-types` (disabled): enable [http-types](https://github.com/http-rs/http-types)
//! type conversion support
//!
//! ### Legacy bincode features (deprecated)
//!
//! These features are deprecated due to [RUSTSEC-2025-0141](https://rustsec.org/advisories/RUSTSEC-2025-0141)
//! and will be removed in the next major version:
//!
//! - `manager-cacache-bincode`: cacache with bincode serialization
//! - `manager-moka-bincode`: moka with bincode serialization
//!
//! **Note**: Only `StreamingManager` (via the `streaming` feature) provides streaming support.
//! Other managers will buffer response bodies in memory even when used with `StreamingManager`.
//!
//! ## Integration
//!
//! This crate is designed to be used as a foundation for HTTP client and server middleware.
//! See the companion crates for specific integrations:
//!
//! - [`http-cache-reqwest`](https://docs.rs/http-cache-reqwest) for reqwest client middleware
//! - [`http-cache-surf`](https://docs.rs/http-cache-surf) for surf client middleware  
//! - [`http-cache-tower`](https://docs.rs/http-cache-tower) for tower service middleware
mod body;
mod error;
mod managers;

#[cfg(feature = "streaming")]
mod runtime;

#[cfg(feature = "rate-limiting")]
pub mod rate_limiting;

use std::{
    collections::HashMap,
    convert::TryFrom,
    fmt::{self, Debug},
    str::FromStr,
    sync::Arc,
    time::{Duration, SystemTime},
};

use http::{
    header::CACHE_CONTROL, request, response, HeaderValue, Response, StatusCode,
};
use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// URL type alias - allows users to choose between `url` (default) and `ada-url` crates
// When using `url-ada` feature, this becomes `ada_url::Url`
#[cfg(feature = "url-ada")]
pub use ada_url::Url;
#[cfg(not(feature = "url-ada"))]
pub use url::Url;

// ============================================================================
// URL Helper Functions
// ============================================================================
// These functions abstract away API differences between `url` and `ada-url` crates.
// Internal code should use these helpers instead of calling URL methods directly.

/// Parse a URL string into a `Url` type.
///
/// This helper abstracts the parsing API difference between `url` and `ada-url`:
/// - `url` crate: `Url::parse(s)` returns `Result<Url, ParseError>`
/// - `ada-url` crate: `Url::parse(s, None)` returns `Result<Url, ParseUrlError>`
#[inline]
pub fn url_parse(s: &str) -> Result<Url> {
    #[cfg(feature = "url-ada")]
    {
        Url::parse(s, None).map_err(|e| -> BoxError { e.to_string().into() })
    }
    #[cfg(not(feature = "url-ada"))]
    {
        Url::parse(s).map_err(|e| -> BoxError { Box::new(e) })
    }
}

/// Set the path component of a URL.
///
/// API differences:
/// - `url` crate: `url.set_path(path)`
/// - `ada-url` crate: `url.set_pathname(Some(path))`
#[inline]
pub fn url_set_path(url: &mut Url, path: &str) {
    #[cfg(feature = "url-ada")]
    {
        let _ = url.set_pathname(Some(path));
    }
    #[cfg(not(feature = "url-ada"))]
    {
        url.set_path(path);
    }
}

/// Set the query component of a URL.
///
/// API differences:
/// - `url` crate: `url.set_query(Some(query))` or `url.set_query(None)`
/// - `ada-url` crate: `url.set_search(Some(query))` or `url.set_search(None)`
#[inline]
pub fn url_set_query(url: &mut Url, query: Option<&str>) {
    #[cfg(feature = "url-ada")]
    {
        url.set_search(query);
    }
    #[cfg(not(feature = "url-ada"))]
    {
        url.set_query(query);
    }
}

/// Get the hostname of a URL as a string.
///
/// API differences:
/// - `url` crate: `url.host_str()` returns `Option<&str>`
/// - `ada-url` crate: `url.hostname()` returns `&str` (empty string if no host)
#[inline]
#[must_use]
pub fn url_hostname(url: &Url) -> Option<&str> {
    #[cfg(feature = "url-ada")]
    {
        let hostname = url.hostname();
        if hostname.is_empty() {
            None
        } else {
            Some(hostname)
        }
    }
    #[cfg(not(feature = "url-ada"))]
    {
        url.host_str()
    }
}

/// Get the host of a URL as a string for display purposes (e.g., warning headers).
///
/// This returns the host portion as a string, or "unknown" if not available.
/// Used in places like HTTP Warning headers where we need a displayable host value.
#[inline]
#[must_use]
pub fn url_host_str(url: &Url) -> String {
    #[cfg(feature = "url-ada")]
    {
        let hostname = url.hostname();
        if hostname.is_empty() {
            "unknown".to_string()
        } else {
            hostname.to_string()
        }
    }
    #[cfg(not(feature = "url-ada"))]
    {
        url.host()
            .map(|h| h.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }
}

pub use body::StreamingBody;
pub use error::{
    BadHeader, BadRequest, BadVersion, BoxError, ClientStreamingError,
    HttpCacheError, HttpCacheResult, Result, StreamingError,
};

#[cfg(feature = "manager-cacache")]
pub use managers::cacache::CACacheManager;

#[cfg(feature = "streaming")]
pub use managers::streaming_cache::StreamingManager;

#[cfg(feature = "manager-moka")]
pub use managers::moka::MokaManager;

#[cfg(feature = "manager-foyer")]
pub use managers::foyer::FoyerManager;

#[cfg(feature = "rate-limiting")]
pub use rate_limiting::{
    CacheAwareRateLimiter, DirectRateLimiter, DomainRateLimiter,
};

#[cfg(feature = "rate-limiting")]
pub use rate_limiting::Quota;

// Exposing the moka cache for convenience, renaming to avoid naming conflicts
#[cfg(feature = "manager-moka")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-moka")))]
pub use moka::future::{Cache as MokaCache, CacheBuilder as MokaCacheBuilder};

// Custom headers used to indicate cache status (hit or miss)
/// `x-cache` header: Value will be HIT if the response was served from cache, MISS if not
pub const XCACHE: &str = "x-cache";
/// `x-cache-lookup` header: Value will be HIT if a response existed in cache, MISS if not
pub const XCACHELOOKUP: &str = "x-cache-lookup";
/// `warning` header: HTTP warning header as per RFC 7234
const WARNING: &str = "warning";

/// Represents a basic cache status
/// Used in the custom headers `x-cache` and `x-cache-lookup`
#[derive(Debug, Copy, Clone)]
pub enum HitOrMiss {
    /// Yes, there was a hit
    HIT,
    /// No, there was no hit
    MISS,
}

impl fmt::Display for HitOrMiss {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::HIT => write!(f, "HIT"),
            Self::MISS => write!(f, "MISS"),
        }
    }
}

/// Represents an HTTP version
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub enum HttpVersion {
    /// HTTP Version 0.9
    #[serde(rename = "HTTP/0.9")]
    Http09,
    /// HTTP Version 1.0
    #[serde(rename = "HTTP/1.0")]
    Http10,
    /// HTTP Version 1.1
    #[serde(rename = "HTTP/1.1")]
    Http11,
    /// HTTP Version 2.0
    #[serde(rename = "HTTP/2.0")]
    H2,
    /// HTTP Version 3.0
    #[serde(rename = "HTTP/3.0")]
    H3,
}

impl fmt::Display for HttpVersion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HttpVersion::Http09 => write!(f, "HTTP/0.9"),
            HttpVersion::Http10 => write!(f, "HTTP/1.0"),
            HttpVersion::Http11 => write!(f, "HTTP/1.1"),
            HttpVersion::H2 => write!(f, "HTTP/2.0"),
            HttpVersion::H3 => write!(f, "HTTP/3.0"),
        }
    }
}

/// Extract a URL from HTTP request parts for cache key generation
///
/// This function reconstructs the full URL from the request parts, handling both
/// HTTP and HTTPS schemes based on the connection type or explicit headers.
fn extract_url_from_request_parts(parts: &request::Parts) -> Result<Url> {
    // First check if the URI is already absolute
    if let Some(_scheme) = parts.uri.scheme() {
        // URI is absolute, use it directly
        return url_parse(&parts.uri.to_string())
            .map_err(|_| -> BoxError { BadHeader.into() });
    }

    // Get the host header
    let host = parts
        .headers
        .get("host")
        .ok_or(BadHeader)?
        .to_str()
        .map_err(|_| BadHeader)?;

    // Determine scheme based on host and headers
    let scheme = determine_scheme(host, &parts.headers)?;

    // Create base URL using the URL helper for cross-crate compatibility
    let mut base_url = url_parse(&format!("{}://{}/", &scheme, host))
        .map_err(|_| -> BoxError { BadHeader.into() })?;

    // Set the path and query from the URI using helpers
    if let Some(path_and_query) = parts.uri.path_and_query() {
        url_set_path(&mut base_url, path_and_query.path());
        if let Some(query) = path_and_query.query() {
            url_set_query(&mut base_url, Some(query));
        }
    }

    Ok(base_url)
}

/// Determine the appropriate scheme for URL construction
fn determine_scheme(host: &str, headers: &http::HeaderMap) -> Result<String> {
    // Check for explicit protocol forwarding header first
    if let Some(forwarded_proto) = headers.get("x-forwarded-proto") {
        let proto = forwarded_proto.to_str().map_err(|_| BadHeader)?;
        return match proto {
            "http" | "https" => Ok(proto.to_string()),
            _ => Ok("https".to_string()), // Default to secure for unknown protocols
        };
    }

    // Check if this looks like a local development host
    if host.starts_with("localhost") || host.starts_with("127.0.0.1") {
        Ok("http".to_string())
    } else {
        Ok("https".to_string()) // Default to secure for all other hosts
    }
}

/// Represents HTTP headers in either legacy or modern format
#[derive(Debug, Clone)]
pub enum HttpHeaders {
    /// Modern header representation - allows multiple values per key
    Modern(HashMap<String, Vec<String>>),
    /// Legacy header representation - kept for backward compatibility with deserialization
    #[cfg(feature = "http-headers-compat")]
    Legacy(HashMap<String, String>),
}

// Serialize directly as the inner HashMap (no enum variant wrapper)
// This ensures compatibility: serialized data is just the raw HashMap
impl Serialize for HttpHeaders {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[cfg(feature = "http-headers-compat")]
        {
            // Always serialize as Legacy format when compat is enabled
            match self {
                HttpHeaders::Modern(modern) => {
                    // Convert Modern to Legacy format by joining values
                    let legacy: HashMap<String, String> = modern
                        .iter()
                        .map(|(k, v)| (k.clone(), v.join(", ")))
                        .collect();
                    legacy.serialize(serializer)
                }
                HttpHeaders::Legacy(legacy) => legacy.serialize(serializer),
            }
        }

        #[cfg(not(feature = "http-headers-compat"))]
        {
            match self {
                HttpHeaders::Modern(modern) => modern.serialize(serializer),
            }
        }
    }
}

// Deserialize directly as HashMap based on feature flag
// With http-headers-compat: reads HashMap<String, String> (legacy alpha.2 format)
// Without http-headers-compat: reads HashMap<String, Vec<String>> (modern format)
impl<'de> Deserialize<'de> for HttpHeaders {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[cfg(feature = "http-headers-compat")]
        {
            let legacy = HashMap::<String, String>::deserialize(deserializer)?;
            Ok(HttpHeaders::Legacy(legacy))
        }

        #[cfg(not(feature = "http-headers-compat"))]
        {
            let modern =
                HashMap::<String, Vec<String>>::deserialize(deserializer)?;
            Ok(HttpHeaders::Modern(modern))
        }
    }
}

impl HttpHeaders {
    /// Creates a new empty HttpHeaders in modern format
    pub fn new() -> Self {
        HttpHeaders::Modern(HashMap::new())
    }

    /// Inserts a header key-value pair, replacing any existing values for that key
    /// Keys are normalized to lowercase per RFC 7230
    pub fn insert(&mut self, key: String, value: String) {
        let normalized_key = key.to_ascii_lowercase();
        match self {
            #[cfg(feature = "http-headers-compat")]
            HttpHeaders::Legacy(legacy) => {
                legacy.insert(normalized_key, value);
            }
            HttpHeaders::Modern(modern) => {
                // Replace existing values with a new single-element vec
                modern.insert(normalized_key, vec![value]);
            }
        }
    }

    /// Appends a header value, preserving existing values for the same key
    /// Keys are normalized to lowercase per RFC 7230
    pub fn append(&mut self, key: String, value: String) {
        let normalized_key = key.to_ascii_lowercase();
        match self {
            #[cfg(feature = "http-headers-compat")]
            HttpHeaders::Legacy(legacy) => {
                // Legacy format doesn't support multi-value, fall back to insert
                legacy.insert(normalized_key, value);
            }
            HttpHeaders::Modern(modern) => {
                modern
                    .entry(normalized_key)
                    .or_insert_with(Vec::new)
                    .push(value);
            }
        }
    }

    /// Retrieves the first value for a given header key
    /// Keys are normalized to lowercase per RFC 7230
    pub fn get(&self, key: &str) -> Option<&String> {
        let normalized_key = key.to_ascii_lowercase();
        match self {
            #[cfg(feature = "http-headers-compat")]
            HttpHeaders::Legacy(legacy) => legacy.get(&normalized_key),
            HttpHeaders::Modern(modern) => {
                modern.get(&normalized_key).and_then(|vals| vals.first())
            }
        }
    }

    /// Removes a header key and its associated values
    /// Keys are normalized to lowercase per RFC 7230
    pub fn remove(&mut self, key: &str) {
        let normalized_key = key.to_ascii_lowercase();
        match self {
            #[cfg(feature = "http-headers-compat")]
            HttpHeaders::Legacy(legacy) => {
                legacy.remove(&normalized_key);
            }
            HttpHeaders::Modern(modern) => {
                modern.remove(&normalized_key);
            }
        }
    }

    /// Checks if a header key exists
    /// Keys are normalized to lowercase per RFC 7230
    pub fn contains_key(&self, key: &str) -> bool {
        let normalized_key = key.to_ascii_lowercase();
        match self {
            #[cfg(feature = "http-headers-compat")]
            HttpHeaders::Legacy(legacy) => legacy.contains_key(&normalized_key),
            HttpHeaders::Modern(modern) => modern.contains_key(&normalized_key),
        }
    }

    /// Returns an iterator over the header key-value pairs
    pub fn iter(&self) -> HttpHeadersIterator<'_> {
        match self {
            #[cfg(feature = "http-headers-compat")]
            HttpHeaders::Legacy(legacy) => {
                HttpHeadersIterator { inner: legacy.iter().collect(), index: 0 }
            }
            HttpHeaders::Modern(modern) => HttpHeadersIterator {
                inner: modern
                    .iter()
                    .flat_map(|(k, vals)| vals.iter().map(move |v| (k, v)))
                    .collect(),
                index: 0,
            },
        }
    }
}

impl From<&http::HeaderMap> for HttpHeaders {
    fn from(headers: &http::HeaderMap) -> Self {
        let mut modern_headers = HashMap::new();

        // Collect all unique header names first
        let header_names: std::collections::HashSet<_> =
            headers.keys().collect();

        // For each header name, collect ALL values
        for name in header_names {
            let values: Vec<String> = headers
                .get_all(name)
                .iter()
                .filter_map(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .collect();

            if !values.is_empty() {
                modern_headers.insert(name.to_string(), values);
            }
        }

        HttpHeaders::Modern(modern_headers)
    }
}

impl From<HttpHeaders> for HashMap<String, Vec<String>> {
    fn from(headers: HttpHeaders) -> Self {
        match headers {
            #[cfg(feature = "http-headers-compat")]
            HttpHeaders::Legacy(legacy) => {
                legacy.into_iter().map(|(k, v)| (k, vec![v])).collect()
            }
            HttpHeaders::Modern(modern) => modern,
        }
    }
}

impl Default for HttpHeaders {
    fn default() -> Self {
        HttpHeaders::new()
    }
}

impl IntoIterator for HttpHeaders {
    type Item = (String, String);
    type IntoIter = HttpHeadersIntoIterator;

    fn into_iter(self) -> Self::IntoIter {
        HttpHeadersIntoIterator {
            inner: match self {
                #[cfg(feature = "http-headers-compat")]
                HttpHeaders::Legacy(legacy) => legacy.into_iter().collect(),
                HttpHeaders::Modern(modern) => modern
                    .into_iter()
                    .flat_map(|(k, vals)| {
                        vals.into_iter().map(move |v| (k.clone(), v))
                    })
                    .collect(),
            },
            index: 0,
        }
    }
}

/// Iterator for HttpHeaders
#[derive(Debug)]
pub struct HttpHeadersIntoIterator {
    inner: Vec<(String, String)>,
    index: usize,
}

impl Iterator for HttpHeadersIntoIterator {
    type Item = (String, String);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.inner.len() {
            let item = self.inner[self.index].clone();
            self.index += 1;
            Some(item)
        } else {
            None
        }
    }
}

impl<'a> IntoIterator for &'a HttpHeaders {
    type Item = (&'a String, &'a String);
    type IntoIter = HttpHeadersIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator for HttpHeaders references
#[derive(Debug)]
pub struct HttpHeadersIterator<'a> {
    inner: Vec<(&'a String, &'a String)>,
    index: usize,
}

impl<'a> Iterator for HttpHeadersIterator<'a> {
    type Item = (&'a String, &'a String);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.inner.len() {
            let item = self.inner[self.index];
            self.index += 1;
            Some(item)
        } else {
            None
        }
    }
}

/// A basic generic type that represents an HTTP response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpResponse {
    /// HTTP response body
    pub body: Vec<u8>,
    /// HTTP response headers
    pub headers: HttpHeaders,
    /// HTTP response status code
    pub status: u16,
    /// HTTP response url
    pub url: Url,
    /// HTTP response version
    pub version: HttpVersion,
    /// Metadata
    #[serde(default)]
    pub metadata: Option<Vec<u8>>,
}

impl HttpResponse {
    /// Returns `http::response::Parts`
    pub fn parts(&self) -> Result<response::Parts> {
        let mut converted =
            response::Builder::new().status(self.status).body(())?;
        {
            let headers = converted.headers_mut();
            for header in &self.headers {
                headers.append(
                    http::header::HeaderName::from_str(header.0.as_str())?,
                    HeaderValue::from_str(header.1.as_str())?,
                );
            }
        }
        Ok(converted.into_parts().0)
    }

    /// Returns the status code of the warning header if present
    #[must_use]
    fn warning_code(&self) -> Option<usize> {
        self.headers.get(WARNING).and_then(|hdr| {
            hdr.as_str().chars().take(3).collect::<String>().parse().ok()
        })
    }

    /// Adds a warning header to a response
    fn add_warning(&mut self, url: &Url, code: usize, message: &str) {
        // warning    = "warning" ":" 1#warning-value
        // warning-value = warn-code SP warn-agent SP warn-text [SP warn-date]
        // warn-code  = 3DIGIT
        // warn-agent = ( host [ ":" port ] ) | pseudonym
        //                 ; the name or pseudonym of the server adding
        //                 ; the warning header, for use in debugging
        // warn-text  = quoted-string
        // warn-date  = <"> HTTP-date <">
        // (https://tools.ietf.org/html/rfc2616#section-14.46)
        let host = url_host_str(url);
        // Escape message to prevent header injection and ensure valid HTTP format
        let escaped_message =
            message.replace('"', "'").replace(['\n', '\r'], " ");
        self.headers.insert(
            WARNING.to_string(),
            format!(
                "{} {} \"{}\" \"{}\"",
                code,
                host,
                escaped_message,
                httpdate::fmt_http_date(SystemTime::now())
            ),
        );
    }

    /// Removes a warning header from a response
    fn remove_warning(&mut self) {
        self.headers.remove(WARNING);
    }

    /// Update the headers from `http::response::Parts`
    pub fn update_headers(&mut self, parts: &response::Parts) -> Result<()> {
        for header in parts.headers.iter() {
            self.headers.insert(
                header.0.as_str().to_string(),
                header.1.to_str()?.to_string(),
            );
        }
        Ok(())
    }

    /// Checks if the Cache-Control header contains the must-revalidate directive
    #[must_use]
    fn must_revalidate(&self) -> bool {
        self.headers.get(CACHE_CONTROL.as_str()).is_some_and(|val| {
            val.as_str().to_lowercase().contains("must-revalidate")
        })
    }

    /// Adds the custom `x-cache` header to the response
    pub fn cache_status(&mut self, hit_or_miss: HitOrMiss) {
        self.headers.insert(XCACHE.to_string(), hit_or_miss.to_string());
    }

    /// Adds the custom `x-cache-lookup` header to the response
    pub fn cache_lookup_status(&mut self, hit_or_miss: HitOrMiss) {
        self.headers.insert(XCACHELOOKUP.to_string(), hit_or_miss.to_string());
    }
}

/// A trait providing methods for storing, reading, and removing cache records.
#[async_trait::async_trait]
pub trait CacheManager: Send + Sync + 'static {
    /// Attempts to pull a cached response and related policy from cache.
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>>;
    /// Attempts to cache a response and related policy.
    async fn put(
        &self,
        cache_key: String,
        res: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse>;
    /// Attempts to remove a record from cache.
    async fn delete(&self, cache_key: &str) -> Result<()>;
}

/// A streaming cache manager that supports streaming request/response bodies
/// without buffering them in memory. This is ideal for large responses.
#[async_trait::async_trait]
pub trait StreamingCacheManager: Send + Sync + 'static {
    /// The body type used by this cache manager
    type Body: http_body::Body + Send + 'static;

    /// Attempts to pull a cached response and related policy from cache with streaming body.
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(Response<Self::Body>, CachePolicy)>>
    where
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static;

    /// Attempts to cache a response with a streaming body and related policy.
    async fn put<B>(
        &self,
        cache_key: String,
        response: Response<B>,
        policy: CachePolicy,
        request_url: Url,
        metadata: Option<Vec<u8>>,
    ) -> Result<Response<Self::Body>>
    where
        B: http_body::Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static;

    /// Converts a generic body to the manager's body type for non-cacheable responses.
    /// This is called when a response should not be cached but still needs to be returned
    /// with the correct body type.
    async fn convert_body<B>(
        &self,
        response: Response<B>,
    ) -> Result<Response<Self::Body>>
    where
        B: http_body::Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static;

    /// Attempts to remove a record from cache.
    async fn delete(&self, cache_key: &str) -> Result<()>;

    /// Creates an empty body of the manager's body type.
    /// Used for returning 504 Gateway Timeout responses on OnlyIfCached cache misses.
    fn empty_body(&self) -> Self::Body;

    /// Convert the manager's body type to a reqwest-compatible bytes stream.
    /// This enables efficient streaming without collecting the entire body.
    #[cfg(feature = "streaming")]
    fn body_to_bytes_stream(
        body: Self::Body,
    ) -> impl futures_util::Stream<
        Item = std::result::Result<
            bytes::Bytes,
            Box<dyn std::error::Error + Send + Sync>,
        >,
    > + Send
    where
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error: Send + Sync + 'static;
}

/// Describes the functionality required for interfacing with HTTP client middleware
#[async_trait::async_trait]
pub trait Middleware: Send {
    /// Allows the cache mode to be overridden.
    ///
    /// This overrides any cache mode set in the configuration, including cache_mode_fn.
    fn overridden_cache_mode(&self) -> Option<CacheMode> {
        None
    }
    /// Determines if the request method is either GET or HEAD
    fn is_method_get_head(&self) -> bool;
    /// Returns a new cache policy with default options
    fn policy(&self, response: &HttpResponse) -> Result<CachePolicy>;
    /// Returns a new cache policy with custom options
    fn policy_with_options(
        &self,
        response: &HttpResponse,
        options: CacheOptions,
    ) -> Result<CachePolicy>;
    /// Attempts to update the request headers with the passed `http::request::Parts`
    fn update_headers(&mut self, parts: &request::Parts) -> Result<()>;
    /// Attempts to force the "no-cache" directive on the request
    fn force_no_cache(&mut self) -> Result<()>;
    /// Attempts to construct `http::request::Parts` from the request
    fn parts(&self) -> Result<request::Parts>;
    /// Attempts to determine the requested url
    fn url(&self) -> Result<Url>;
    /// Attempts to determine the request method
    fn method(&self) -> Result<String>;
    /// Attempts to fetch an upstream resource and return an [`HttpResponse`]
    async fn remote_fetch(&mut self) -> Result<HttpResponse>;
}

/// An interface for HTTP caching that works with composable middleware patterns
/// like Tower. This trait separates the concerns of request analysis, cache lookup,
/// and response processing into discrete steps.
pub trait HttpCacheInterface<B = Vec<u8>>: Send + Sync {
    /// Analyze a request to determine cache behavior
    fn analyze_request(
        &self,
        parts: &request::Parts,
        mode_override: Option<CacheMode>,
    ) -> Result<CacheAnalysis>;

    /// Look up a cached response for the given cache key
    #[allow(async_fn_in_trait)]
    async fn lookup_cached_response(
        &self,
        key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>>;

    /// Process a fresh response from upstream and potentially cache it
    #[allow(async_fn_in_trait)]
    async fn process_response(
        &self,
        analysis: CacheAnalysis,
        response: Response<B>,
        metadata: Option<Vec<u8>>,
    ) -> Result<Response<B>>;

    /// Update request headers for conditional requests (e.g., If-None-Match)
    fn prepare_conditional_request(
        &self,
        parts: &mut request::Parts,
        cached_response: &HttpResponse,
        policy: &CachePolicy,
    ) -> Result<()>;

    /// Handle a 304 Not Modified response by returning the cached response
    #[allow(async_fn_in_trait)]
    async fn handle_not_modified(
        &self,
        cached_response: HttpResponse,
        fresh_parts: &response::Parts,
    ) -> Result<HttpResponse>;
}

/// Streaming version of the HTTP cache interface that supports streaming request/response bodies
/// without buffering them in memory. This is ideal for large responses or when memory usage
/// is a concern.
pub trait HttpCacheStreamInterface: Send + Sync {
    /// The body type used by this cache implementation
    type Body: http_body::Body + Send + 'static;

    /// Analyze a request to determine cache behavior
    fn analyze_request(
        &self,
        parts: &request::Parts,
        mode_override: Option<CacheMode>,
    ) -> Result<CacheAnalysis>;

    /// Look up a cached response for the given cache key, returning a streaming body
    #[allow(async_fn_in_trait)]
    async fn lookup_cached_response(
        &self,
        key: &str,
    ) -> Result<Option<(Response<Self::Body>, CachePolicy)>>
    where
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static;

    /// Process a fresh response from upstream and potentially cache it with streaming support
    #[allow(async_fn_in_trait)]
    async fn process_response<B>(
        &self,
        analysis: CacheAnalysis,
        response: Response<B>,
        metadata: Option<Vec<u8>>,
    ) -> Result<Response<Self::Body>>
    where
        B: http_body::Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static;

    /// Update request headers for conditional requests (e.g., If-None-Match)
    fn prepare_conditional_request(
        &self,
        parts: &mut request::Parts,
        cached_response: &Response<Self::Body>,
        policy: &CachePolicy,
    ) -> Result<()>;

    /// Handle a 304 Not Modified response by returning the cached response
    #[allow(async_fn_in_trait)]
    async fn handle_not_modified(
        &self,
        cached_response: Response<Self::Body>,
        fresh_parts: &response::Parts,
    ) -> Result<Response<Self::Body>>
    where
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static;
}

/// Analysis result for a request, containing cache key and caching decisions
#[derive(Debug, Clone)]
pub struct CacheAnalysis {
    /// The cache key for this request
    pub cache_key: String,
    /// Whether this request should be cached
    pub should_cache: bool,
    /// The effective cache mode for this request
    pub cache_mode: CacheMode,
    /// Keys to bust from cache before processing
    pub cache_bust_keys: Vec<String>,
    /// The request parts for policy creation
    pub request_parts: request::Parts,
    /// Whether this is a GET or HEAD request
    pub is_get_head: bool,
}

/// Cache mode determines how the HTTP cache behaves for requests.
///
/// These modes are similar to [make-fetch-happen cache options](https://github.com/npm/make-fetch-happen#--optscache)
/// and provide fine-grained control over caching behavior.
///
/// # Examples
///
/// ```rust
/// # #[cfg(feature = "manager-cacache")]
/// # fn main() {
/// use http_cache::{CacheMode, HttpCache, CACacheManager, HttpCacheOptions};
///
/// let manager = CACacheManager::new("./cache".into(), true);
///
/// // Use different cache modes for different scenarios
/// let default_cache = HttpCache {
///     mode: CacheMode::Default,        // Standard HTTP caching rules
///     manager: manager.clone(),
///     options: HttpCacheOptions::default(),
/// };
///
/// let force_cache = HttpCache {
///     mode: CacheMode::ForceCache,     // Cache everything, ignore staleness
///     manager: manager.clone(),
///     options: HttpCacheOptions::default(),
/// };
///
/// let no_cache = HttpCache {
///     mode: CacheMode::NoStore,        // Never cache anything
///     manager,
///     options: HttpCacheOptions::default(),
/// };
/// # }
/// # #[cfg(not(feature = "manager-cacache"))]
/// # fn main() {}
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    /// Standard HTTP caching behavior (recommended for most use cases).
    ///
    /// This mode:
    /// - Checks the cache for fresh responses and uses them
    /// - Makes conditional requests for stale responses (revalidation)
    /// - Makes normal requests when no cached response exists
    /// - Updates the cache with new responses
    /// - Falls back to stale responses if revalidation fails
    ///
    /// This is the most common mode and follows HTTP caching standards closely.
    #[default]
    Default,

    /// Completely bypasses the cache.
    ///
    /// This mode:
    /// - Never reads from the cache
    /// - Never writes to the cache
    /// - Always makes fresh network requests
    ///
    /// Use this when you need to ensure every request goes to the origin server.
    NoStore,

    /// Bypasses cache on request but updates cache with response.
    ///
    /// This mode:
    /// - Ignores any cached responses
    /// - Always makes a fresh network request
    /// - Updates the cache with the response
    ///
    /// Equivalent to a "hard refresh" - useful when you know the cache is stale.
    Reload,

    /// Always revalidates cached responses.
    ///
    /// This mode:
    /// - Makes conditional requests if a cached response exists
    /// - Makes normal requests if no cached response exists
    /// - Updates the cache with responses
    ///
    /// Use this when you want to ensure content freshness while still benefiting
    /// from conditional requests (304 Not Modified responses).
    NoCache,

    /// Uses cached responses regardless of staleness.
    ///
    /// This mode:
    /// - Uses any cached response, even if stale
    /// - Makes network requests only when no cached response exists
    /// - Updates the cache with new responses
    ///
    /// Useful for offline scenarios or when performance is more important than freshness.
    ForceCache,

    /// Only serves from cache, never makes network requests.
    ///
    /// This mode:
    /// - Uses any cached response, even if stale
    /// - Returns an error if no cached response exists
    /// - Never makes network requests
    ///
    /// Use this for offline-only scenarios or when you want to guarantee
    /// no network traffic.
    OnlyIfCached,

    /// Ignores HTTP caching rules and caches everything.
    ///
    /// This mode:
    /// - Caches all 200 responses regardless of cache-control headers
    /// - Uses cached responses regardless of staleness
    /// - Makes network requests when no cached response exists
    ///
    /// Use this when you want aggressive caching and don't want to respect
    /// server cache directives.
    IgnoreRules,
}

impl TryFrom<http::Version> for HttpVersion {
    type Error = BoxError;

    fn try_from(value: http::Version) -> Result<Self> {
        Ok(match value {
            http::Version::HTTP_09 => Self::Http09,
            http::Version::HTTP_10 => Self::Http10,
            http::Version::HTTP_11 => Self::Http11,
            http::Version::HTTP_2 => Self::H2,
            http::Version::HTTP_3 => Self::H3,
            _ => return Err(Box::new(BadVersion)),
        })
    }
}

impl From<HttpVersion> for http::Version {
    fn from(value: HttpVersion) -> Self {
        match value {
            HttpVersion::Http09 => Self::HTTP_09,
            HttpVersion::Http10 => Self::HTTP_10,
            HttpVersion::Http11 => Self::HTTP_11,
            HttpVersion::H2 => Self::HTTP_2,
            HttpVersion::H3 => Self::HTTP_3,
        }
    }
}

#[cfg(feature = "http-types")]
impl TryFrom<http_types::Version> for HttpVersion {
    type Error = BoxError;

    fn try_from(value: http_types::Version) -> Result<Self> {
        Ok(match value {
            http_types::Version::Http0_9 => Self::Http09,
            http_types::Version::Http1_0 => Self::Http10,
            http_types::Version::Http1_1 => Self::Http11,
            http_types::Version::Http2_0 => Self::H2,
            http_types::Version::Http3_0 => Self::H3,
            _ => return Err(Box::new(BadVersion)),
        })
    }
}

#[cfg(feature = "http-types")]
impl From<HttpVersion> for http_types::Version {
    fn from(value: HttpVersion) -> Self {
        match value {
            HttpVersion::Http09 => Self::Http0_9,
            HttpVersion::Http10 => Self::Http1_0,
            HttpVersion::Http11 => Self::Http1_1,
            HttpVersion::H2 => Self::Http2_0,
            HttpVersion::H3 => Self::Http3_0,
        }
    }
}

/// Options struct provided by
/// [`http-cache-semantics`](https://github.com/kornelski/rusty-http-cache-semantics).
pub use http_cache_semantics::CacheOptions;

/// A closure that takes [`http::request::Parts`] and returns a [`String`].
/// By default, the cache key is a combination of the request method and uri with a colon in between.
pub type CacheKey = Arc<dyn Fn(&request::Parts) -> String + Send + Sync>;

/// A closure that takes [`http::request::Parts`] and returns a [`CacheMode`]
pub type CacheModeFn = Arc<dyn Fn(&request::Parts) -> CacheMode + Send + Sync>;

/// A closure that takes [`http::request::Parts`], [`HttpResponse`] and returns a [`CacheMode`] to override caching behavior based on the response
pub type ResponseCacheModeFn = Arc<
    dyn Fn(&request::Parts, &HttpResponse) -> Option<CacheMode> + Send + Sync,
>;

/// A closure that takes [`http::request::Parts`], [`Option<CacheKey>`], the default cache key ([`&str`]) and returns [`Vec<String>`] of keys to bust the cache for.
/// An empty vector means that no cache busting will be performed.
pub type CacheBust = Arc<
    dyn Fn(&request::Parts, &Option<CacheKey>, &str) -> Vec<String>
        + Send
        + Sync,
>;

/// Type alias for metadata stored alongside cached responses.
/// Users are responsible for serialization/deserialization of this data.
pub type HttpCacheMetadata = Vec<u8>;

/// A closure that takes [`http::request::Parts`] and [`http::response::Parts`] and returns optional metadata to store with the cached response.
/// This allows middleware to compute and store additional information alongside cached responses.
pub type MetadataProvider = Arc<
    dyn Fn(&request::Parts, &response::Parts) -> Option<HttpCacheMetadata>
        + Send
        + Sync,
>;

/// A closure that takes a mutable reference to [`HttpResponse`] and modifies it before caching.
pub type ModifyResponse = Arc<dyn Fn(&mut HttpResponse) + Send + Sync>;

/// Configuration options for customizing HTTP cache behavior on a per-request basis.
///
/// This struct allows you to override default caching behavior for individual requests
/// by providing custom cache options, cache keys, cache modes, and cache busting logic.
///
/// # Examples
///
/// ## Basic Custom Cache Key
/// ```rust
/// use http_cache::{HttpCacheOptions, CacheKey};
/// use http::request::Parts;
/// use std::sync::Arc;
///
/// let options = HttpCacheOptions {
///     cache_key: Some(Arc::new(|parts: &Parts| {
///         format!("custom:{}:{}", parts.method, parts.uri.path())
///     })),
///     ..Default::default()
/// };
/// ```
///
/// ## Custom Cache Mode per Request
/// ```rust
/// use http_cache::{HttpCacheOptions, CacheMode, CacheModeFn};
/// use http::request::Parts;
/// use std::sync::Arc;
///
/// let options = HttpCacheOptions {
///     cache_mode_fn: Some(Arc::new(|parts: &Parts| {
///         if parts.headers.contains_key("x-no-cache") {
///             CacheMode::NoStore
///         } else {
///             CacheMode::Default
///         }
///     })),
///     ..Default::default()
/// };
/// ```
///
/// ## Response-Based Cache Mode Override
/// ```rust
/// use http_cache::{HttpCacheOptions, ResponseCacheModeFn, CacheMode};
/// use http::request::Parts;
/// use http_cache::HttpResponse;
/// use std::sync::Arc;
///
/// let options = HttpCacheOptions {
///     response_cache_mode_fn: Some(Arc::new(|_parts: &Parts, response: &HttpResponse| {
///         // Force cache 2xx responses even if headers say not to cache
///         if response.status >= 200 && response.status < 300 {
///             Some(CacheMode::ForceCache)
///         } else if response.status == 429 { // Rate limited
///             Some(CacheMode::NoStore) // Don't cache rate limit responses
///         } else {
///             None // Use default behavior
///         }
///     })),
///     ..Default::default()
/// };
/// ```
///
/// ## Content-Type Based Cache Mode Override
/// ```rust
/// use http_cache::{HttpCacheOptions, ResponseCacheModeFn, CacheMode};
/// use http::request::Parts;
/// use http_cache::HttpResponse;
/// use std::sync::Arc;
///
/// let options = HttpCacheOptions {
///     response_cache_mode_fn: Some(Arc::new(|_parts: &Parts, response: &HttpResponse| {
///         // Cache different content types with different strategies
///         if let Some(content_type) = response.headers.get("content-type") {
///             match content_type.as_str() {
///                 ct if ct.starts_with("application/json") => Some(CacheMode::ForceCache),
///                 ct if ct.starts_with("image/") => Some(CacheMode::Default),
///                 ct if ct.starts_with("text/html") => Some(CacheMode::NoStore),
///                 _ => None, // Use default behavior for other types
///             }
///         } else {
///             Some(CacheMode::NoStore) // No content-type = don't cache
///         }
///     })),
///     ..Default::default()
/// };
/// ```
///
/// ## Cache Busting for Related Resources
/// ```rust
/// use http_cache::{HttpCacheOptions, CacheBust, CacheKey};
/// use http::request::Parts;
/// use std::sync::Arc;
///
/// let options = HttpCacheOptions {
///     cache_bust: Some(Arc::new(|parts: &Parts, _cache_key: &Option<CacheKey>, _uri: &str| {
///         if parts.method == "POST" && parts.uri.path().starts_with("/api/users") {
///             vec![
///                 "GET:/api/users".to_string(),
///                 "GET:/api/users/list".to_string(),
///             ]
///         } else {
///             vec![]
///         }
///     })),
///     ..Default::default()
/// };
/// ```
///
/// ## Storing Metadata with Cached Responses
/// ```rust
/// use http_cache::{HttpCacheOptions, MetadataProvider};
/// use http::{request, response};
/// use std::sync::Arc;
///
/// let options = HttpCacheOptions {
///     metadata_provider: Some(Arc::new(|request_parts: &request::Parts, response_parts: &response::Parts| {
///         // Store computed information with the cached response
///         let content_type = response_parts
///             .headers
///             .get("content-type")
///             .and_then(|v| v.to_str().ok())
///             .unwrap_or("unknown");
///
///         // Return serialized metadata (users handle serialization)
///         Some(format!("path={};content-type={}", request_parts.uri.path(), content_type).into_bytes())
///     })),
///     ..Default::default()
/// };
/// ```
#[derive(Clone)]
pub struct HttpCacheOptions {
    /// Override the default cache options.
    pub cache_options: Option<CacheOptions>,
    /// Override the default cache key generator.
    pub cache_key: Option<CacheKey>,
    /// Override the default cache mode.
    pub cache_mode_fn: Option<CacheModeFn>,
    /// Override cache behavior based on the response received.
    /// This function is called after receiving a response and can override
    /// the cache mode for that specific response. Returning `None` means
    /// use the default cache mode. This allows fine-grained control over
    /// caching behavior based on response status, headers, or content.
    pub response_cache_mode_fn: Option<ResponseCacheModeFn>,
    /// Bust the caches of the returned keys.
    pub cache_bust: Option<CacheBust>,
    /// Modifies the response before storing it in the cache.
    pub modify_response: Option<ModifyResponse>,
    /// Determines if the cache status headers should be added to the response.
    pub cache_status_headers: bool,
    /// Maximum time-to-live for cached responses.
    /// When set, this overrides any longer cache durations specified by the server.
    /// Particularly useful with `CacheMode::IgnoreRules` to provide expiration control.
    pub max_ttl: Option<Duration>,
    /// Rate limiter that applies only on cache misses.
    /// When enabled, requests that result in cache hits are returned immediately,
    /// while cache misses are rate limited before making network requests.
    /// This provides the optimal behavior for web scrapers and similar applications.
    #[cfg(feature = "rate-limiting")]
    pub rate_limiter: Option<Arc<dyn CacheAwareRateLimiter>>,
    /// Optional callback to provide metadata to store alongside cached responses.
    /// The callback receives request and response parts and can return metadata bytes.
    /// This is useful for storing computed information that should be associated with
    /// cached responses without recomputation on cache hits.
    pub metadata_provider: Option<MetadataProvider>,
}

impl Default for HttpCacheOptions {
    fn default() -> Self {
        Self {
            cache_options: None,
            cache_key: None,
            cache_mode_fn: None,
            response_cache_mode_fn: None,
            cache_bust: None,
            modify_response: None,
            cache_status_headers: true,
            max_ttl: None,
            #[cfg(feature = "rate-limiting")]
            rate_limiter: None,
            metadata_provider: None,
        }
    }
}

impl Debug for HttpCacheOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(feature = "rate-limiting")]
        {
            f.debug_struct("HttpCacheOptions")
                .field("cache_options", &self.cache_options)
                .field("cache_key", &"Fn(&request::Parts) -> String")
                .field("cache_mode_fn", &"Fn(&request::Parts) -> CacheMode")
                .field(
                    "response_cache_mode_fn",
                    &"Fn(&request::Parts, &HttpResponse) -> Option<CacheMode>",
                )
                .field("cache_bust", &"Fn(&request::Parts) -> Vec<String>")
                .field("modify_response", &"Fn(&mut ModifyResponse)")
                .field("cache_status_headers", &self.cache_status_headers)
                .field("max_ttl", &self.max_ttl)
                .field("rate_limiter", &"Option<CacheAwareRateLimiter>")
                .field(
                    "metadata_provider",
                    &"Fn(&request::Parts, &response::Parts) -> Option<Vec<u8>>",
                )
                .finish()
        }

        #[cfg(not(feature = "rate-limiting"))]
        {
            f.debug_struct("HttpCacheOptions")
                .field("cache_options", &self.cache_options)
                .field("cache_key", &"Fn(&request::Parts) -> String")
                .field("cache_mode_fn", &"Fn(&request::Parts) -> CacheMode")
                .field(
                    "response_cache_mode_fn",
                    &"Fn(&request::Parts, &HttpResponse) -> Option<CacheMode>",
                )
                .field("cache_bust", &"Fn(&request::Parts) -> Vec<String>")
                .field("modify_response", &"Fn(&mut ModifyResponse)")
                .field("cache_status_headers", &self.cache_status_headers)
                .field("max_ttl", &self.max_ttl)
                .field(
                    "metadata_provider",
                    &"Fn(&request::Parts, &response::Parts) -> Option<Vec<u8>>",
                )
                .finish()
        }
    }
}

impl HttpCacheOptions {
    fn create_cache_key(
        &self,
        parts: &request::Parts,
        override_method: Option<&str>,
    ) -> String {
        if let Some(cache_key) = &self.cache_key {
            cache_key(parts)
        } else {
            format!(
                "{}:{}",
                override_method.unwrap_or_else(|| parts.method.as_str()),
                parts.uri
            )
        }
    }

    /// Helper function for other crates to generate cache keys for invalidation
    /// This ensures consistent cache key generation across all implementations
    pub fn create_cache_key_for_invalidation(
        &self,
        parts: &request::Parts,
        method_override: &str,
    ) -> String {
        self.create_cache_key(parts, Some(method_override))
    }

    /// Converts HttpResponse to http::Response with the given body type
    pub fn http_response_to_response<B>(
        http_response: &HttpResponse,
        body: B,
    ) -> Result<Response<B>> {
        let mut response_builder = Response::builder()
            .status(http_response.status)
            .version(http_response.version.into());

        for (name, value) in &http_response.headers {
            if let (Ok(header_name), Ok(header_value)) =
                (name.parse::<http::HeaderName>(), value.parse::<HeaderValue>())
            {
                response_builder =
                    response_builder.header(header_name, header_value);
            }
        }

        Ok(response_builder.body(body)?)
    }

    /// Converts response parts to HttpResponse format for cache mode evaluation
    fn parts_to_http_response(
        &self,
        parts: &response::Parts,
        request_parts: &request::Parts,
        metadata: Option<Vec<u8>>,
    ) -> Result<HttpResponse> {
        Ok(HttpResponse {
            body: vec![], // We don't need the full body for cache mode decision
            headers: (&parts.headers).into(),
            status: parts.status.as_u16(),
            url: extract_url_from_request_parts(request_parts)?,
            version: parts.version.try_into()?,
            metadata,
        })
    }

    /// Evaluates response-based cache mode override
    fn evaluate_response_cache_mode(
        &self,
        request_parts: &request::Parts,
        http_response: &HttpResponse,
        original_mode: CacheMode,
    ) -> CacheMode {
        if let Some(response_cache_mode_fn) = &self.response_cache_mode_fn {
            if let Some(override_mode) =
                response_cache_mode_fn(request_parts, http_response)
            {
                return override_mode;
            }
        }
        original_mode
    }

    /// Generates metadata for a response using the metadata_provider callback if configured
    pub fn generate_metadata(
        &self,
        request_parts: &request::Parts,
        response_parts: &response::Parts,
    ) -> Option<HttpCacheMetadata> {
        self.metadata_provider
            .as_ref()
            .and_then(|provider| provider(request_parts, response_parts))
    }

    /// Modifies the response before caching if a modifier function is provided
    fn modify_response_before_caching(&self, response: &mut HttpResponse) {
        if let Some(modify_response) = &self.modify_response {
            modify_response(response);
        }
    }

    /// Creates a cache policy for the given request and response
    fn create_cache_policy(
        &self,
        request_parts: &request::Parts,
        response_parts: &response::Parts,
    ) -> CachePolicy {
        let cache_options = self.cache_options.unwrap_or_default();

        // If max_ttl is specified, we need to modify the response headers to enforce it
        if let Some(max_ttl) = self.max_ttl {
            // Parse existing cache-control header
            let cache_control = response_parts
                .headers
                .get("cache-control")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            // Extract existing max-age if present
            let existing_max_age =
                cache_control.split(',').find_map(|directive| {
                    let directive = directive.trim();
                    if directive.starts_with("max-age=") {
                        directive.strip_prefix("max-age=")?.parse::<u64>().ok()
                    } else {
                        None
                    }
                });

            // Convert max_ttl to seconds
            let max_ttl_seconds = max_ttl.as_secs();

            // Apply max_ttl by setting max-age to the minimum of existing max-age and max_ttl
            let effective_max_age = match existing_max_age {
                Some(existing) => std::cmp::min(existing, max_ttl_seconds),
                None => max_ttl_seconds,
            };

            // Build new cache-control header
            let mut new_directives = Vec::new();

            // Add non-max-age directives from existing cache-control
            for directive in cache_control.split(',').map(|d| d.trim()) {
                if !directive.starts_with("max-age=") && !directive.is_empty() {
                    new_directives.push(directive.to_string());
                }
            }

            // Add our effective max-age
            new_directives.push(format!("max-age={}", effective_max_age));

            let new_cache_control = new_directives.join(", ");

            // Create modified response parts - we have to clone since response::Parts has private fields
            let mut modified_response_parts = response_parts.clone();
            modified_response_parts.headers.insert(
                "cache-control",
                HeaderValue::from_str(&new_cache_control)
                    .unwrap_or_else(|_| HeaderValue::from_static("max-age=0")),
            );

            CachePolicy::new_options(
                request_parts,
                &modified_response_parts,
                SystemTime::now(),
                cache_options,
            )
        } else {
            CachePolicy::new_options(
                request_parts,
                response_parts,
                SystemTime::now(),
                cache_options,
            )
        }
    }

    /// Determines if a response should be cached based on cache mode and HTTP semantics
    fn should_cache_response(
        &self,
        effective_cache_mode: CacheMode,
        http_response: &HttpResponse,
        is_get_head: bool,
        policy: &CachePolicy,
    ) -> bool {
        // HTTP status codes that are cacheable by default (RFC 7234)
        let is_cacheable_status = matches!(
            http_response.status,
            200 | 203 | 204 | 206 | 300 | 301 | 404 | 405 | 410 | 414 | 501
        );

        if is_cacheable_status {
            match effective_cache_mode {
                CacheMode::ForceCache => is_get_head,
                CacheMode::IgnoreRules => true,
                CacheMode::NoStore => false,
                _ => is_get_head && policy.is_storable(),
            }
        } else {
            false
        }
    }

    /// Common request analysis logic shared between streaming and non-streaming implementations
    fn analyze_request_internal(
        &self,
        parts: &request::Parts,
        mode_override: Option<CacheMode>,
        default_mode: CacheMode,
    ) -> Result<CacheAnalysis> {
        let effective_mode = mode_override
            .or_else(|| self.cache_mode_fn.as_ref().map(|f| f(parts)))
            .unwrap_or(default_mode);

        let is_get_head = parts.method == "GET" || parts.method == "HEAD";
        let should_cache = effective_mode == CacheMode::IgnoreRules
            || (is_get_head && effective_mode != CacheMode::NoStore);

        let cache_key = self.create_cache_key(parts, None);

        let cache_bust_keys = if let Some(cache_bust) = &self.cache_bust {
            cache_bust(parts, &self.cache_key, &cache_key)
        } else {
            Vec::new()
        };

        Ok(CacheAnalysis {
            cache_key,
            should_cache,
            cache_mode: effective_mode,
            cache_bust_keys,
            request_parts: parts.clone(),
            is_get_head,
        })
    }
}

/// Caches requests according to http spec.
#[derive(Debug, Clone)]
pub struct HttpCache<T: CacheManager> {
    /// Determines the manager behavior.
    pub mode: CacheMode,
    /// Manager instance that implements the [`CacheManager`] trait.
    /// By default, a manager implementation with [`cacache`](https://github.com/zkat/cacache-rs)
    /// as the backend has been provided, see [`CACacheManager`].
    pub manager: T,
    /// Override the default cache options.
    pub options: HttpCacheOptions,
}

/// Streaming version of HTTP cache that supports streaming request/response bodies
/// without buffering them in memory.
#[derive(Debug, Clone)]
pub struct HttpStreamingCache<T: StreamingCacheManager> {
    /// Determines the manager behavior.
    pub mode: CacheMode,
    /// Manager instance that implements the [`StreamingCacheManager`] trait.
    pub manager: T,
    /// Override the default cache options.
    pub options: HttpCacheOptions,
}

#[allow(dead_code)]
impl<T: CacheManager> HttpCache<T> {
    /// Determines if the request should be cached
    pub fn can_cache_request(
        &self,
        middleware: &impl Middleware,
    ) -> Result<bool> {
        let analysis = self.analyze_request(
            &middleware.parts()?,
            middleware.overridden_cache_mode(),
        )?;
        Ok(analysis.should_cache)
    }

    /// Apply rate limiting if enabled in options
    #[cfg(feature = "rate-limiting")]
    async fn apply_rate_limiting(&self, url: &Url) {
        if let Some(rate_limiter) = &self.options.rate_limiter {
            let rate_limit_key = url_hostname(url).unwrap_or("unknown");
            rate_limiter.until_key_ready(rate_limit_key).await;
        }
    }

    /// Apply rate limiting if enabled in options (no-op without rate-limiting feature)
    #[cfg(not(feature = "rate-limiting"))]
    async fn apply_rate_limiting(&self, _url: &Url) {
        // No-op when rate limiting feature is not enabled
    }

    /// Runs the actions to preform when the client middleware is running without the cache
    pub async fn run_no_cache(
        &self,
        middleware: &mut impl Middleware,
    ) -> Result<()> {
        let parts = middleware.parts()?;

        self.manager
            .delete(&self.options.create_cache_key(&parts, Some("GET")))
            .await
            .ok();

        let cache_key = self.options.create_cache_key(&parts, None);

        if let Some(cache_bust) = &self.options.cache_bust {
            for key_to_cache_bust in
                cache_bust(&parts, &self.options.cache_key, &cache_key)
            {
                self.manager.delete(&key_to_cache_bust).await?;
            }
        }

        Ok(())
    }

    /// Attempts to run the passed middleware along with the cache
    pub async fn run(
        &self,
        mut middleware: impl Middleware,
    ) -> Result<HttpResponse> {
        // Use the HttpCacheInterface to analyze the request
        let analysis = self.analyze_request(
            &middleware.parts()?,
            middleware.overridden_cache_mode(),
        )?;

        if !analysis.should_cache {
            return self.remote_fetch(&mut middleware).await;
        }

        // Bust cache keys if needed
        for key in &analysis.cache_bust_keys {
            self.manager.delete(key).await?;
        }

        // Look up cached response
        if let Some((mut cached_response, policy)) =
            self.lookup_cached_response(&analysis.cache_key).await?
        {
            if self.options.cache_status_headers {
                cached_response.cache_lookup_status(HitOrMiss::HIT);
            }

            // Handle warning headers
            if let Some(warning_code) = cached_response.warning_code() {
                // https://tools.ietf.org/html/rfc7234#section-4.3.4
                //
                // If a stored response is selected for update, the cache MUST:
                //
                // * delete any warning header fields in the stored response with
                //   warn-code 1xx (see Section 5.5);
                //
                // * retain any warning header fields in the stored response with
                //   warn-code 2xx;
                //
                if (100..200).contains(&warning_code) {
                    cached_response.remove_warning();
                }
            }

            match analysis.cache_mode {
                CacheMode::Default => {
                    self.conditional_fetch(middleware, cached_response, policy)
                        .await
                }
                CacheMode::NoCache => {
                    middleware.force_no_cache()?;
                    let mut res = self.remote_fetch(&mut middleware).await?;
                    if self.options.cache_status_headers {
                        res.cache_lookup_status(HitOrMiss::HIT);
                    }
                    Ok(res)
                }
                CacheMode::ForceCache
                | CacheMode::OnlyIfCached
                | CacheMode::IgnoreRules => {
                    //   112 Disconnected operation
                    // SHOULD be included if the cache is intentionally disconnected from
                    // the rest of the network for a period of time.
                    // (https://tools.ietf.org/html/rfc2616#section-14.46)
                    cached_response.add_warning(
                        &cached_response.url.clone(),
                        112,
                        "Disconnected operation",
                    );
                    if self.options.cache_status_headers {
                        cached_response.cache_status(HitOrMiss::HIT);
                    }
                    Ok(cached_response)
                }
                _ => self.remote_fetch(&mut middleware).await,
            }
        } else {
            match analysis.cache_mode {
                CacheMode::OnlyIfCached => {
                    // ENOTCACHED
                    let mut res = HttpResponse {
                        body: b"GatewayTimeout".to_vec(),
                        headers: HttpHeaders::default(),
                        status: 504,
                        url: middleware.url()?,
                        version: HttpVersion::Http11,
                        metadata: None,
                    };
                    if self.options.cache_status_headers {
                        res.cache_status(HitOrMiss::MISS);
                        res.cache_lookup_status(HitOrMiss::MISS);
                    }
                    Ok(res)
                }
                _ => self.remote_fetch(&mut middleware).await,
            }
        }
    }

    fn cache_mode(&self, middleware: &impl Middleware) -> Result<CacheMode> {
        Ok(if let Some(mode) = middleware.overridden_cache_mode() {
            mode
        } else if let Some(cache_mode_fn) = &self.options.cache_mode_fn {
            cache_mode_fn(&middleware.parts()?)
        } else {
            self.mode
        })
    }

    async fn remote_fetch(
        &self,
        middleware: &mut impl Middleware,
    ) -> Result<HttpResponse> {
        // Apply rate limiting before making the network request
        let url = middleware.url()?;
        self.apply_rate_limiting(&url).await;

        let mut res = middleware.remote_fetch().await?;
        if self.options.cache_status_headers {
            res.cache_status(HitOrMiss::MISS);
            res.cache_lookup_status(HitOrMiss::MISS);
        }
        let policy = match self.options.cache_options {
            Some(options) => middleware.policy_with_options(&res, options)?,
            None => middleware.policy(&res)?,
        };
        let is_get_head = middleware.is_method_get_head();
        let mut mode = self.cache_mode(middleware)?;
        let parts = middleware.parts()?;

        // Allow response-based cache mode override
        if let Some(response_cache_mode_fn) =
            &self.options.response_cache_mode_fn
        {
            if let Some(override_mode) = response_cache_mode_fn(&parts, &res) {
                mode = override_mode;
            }
        }

        let is_cacheable = self.options.should_cache_response(
            mode,
            &res,
            is_get_head,
            &policy,
        );

        if is_cacheable {
            // Generate metadata using the provider callback if configured
            let response_parts = res.parts()?;
            res.metadata =
                self.options.generate_metadata(&parts, &response_parts);

            self.options.modify_response_before_caching(&mut res);
            Ok(self
                .manager
                .put(self.options.create_cache_key(&parts, None), res, policy)
                .await?)
        } else if !is_get_head {
            self.manager
                .delete(&self.options.create_cache_key(&parts, Some("GET")))
                .await
                .ok();
            Ok(res)
        } else {
            Ok(res)
        }
    }

    async fn conditional_fetch(
        &self,
        mut middleware: impl Middleware,
        mut cached_res: HttpResponse,
        mut policy: CachePolicy,
    ) -> Result<HttpResponse> {
        let parts = middleware.parts()?;
        let before_req = policy.before_request(&parts, SystemTime::now());
        match before_req {
            BeforeRequest::Fresh(parts) => {
                cached_res.update_headers(&parts)?;
                if self.options.cache_status_headers {
                    cached_res.cache_status(HitOrMiss::HIT);
                    cached_res.cache_lookup_status(HitOrMiss::HIT);
                }
                return Ok(cached_res);
            }
            BeforeRequest::Stale { request: parts, matches } => {
                if matches {
                    middleware.update_headers(&parts)?;
                }
            }
        }
        let req_url = middleware.url()?;
        // Apply rate limiting before revalidation request
        self.apply_rate_limiting(&req_url).await;
        match middleware.remote_fetch().await {
            Ok(mut cond_res) => {
                let status = StatusCode::from_u16(cond_res.status)?;
                if status.is_server_error() && cached_res.must_revalidate() {
                    //   111 Revalidation failed
                    //   MUST be included if a cache returns a stale response
                    //   because an attempt to revalidate the response failed,
                    //   due to an inability to reach the server.
                    // (https://tools.ietf.org/html/rfc2616#section-14.46)
                    cached_res.add_warning(
                        &req_url,
                        111,
                        "Revalidation failed",
                    );
                    if self.options.cache_status_headers {
                        cached_res.cache_status(HitOrMiss::HIT);
                    }
                    Ok(cached_res)
                } else if cond_res.status == 304 {
                    let after_res = policy.after_response(
                        &parts,
                        &cond_res.parts()?,
                        SystemTime::now(),
                    );
                    match after_res {
                        AfterResponse::Modified(new_policy, parts)
                        | AfterResponse::NotModified(new_policy, parts) => {
                            policy = new_policy;
                            cached_res.update_headers(&parts)?;
                        }
                    }
                    if self.options.cache_status_headers {
                        cached_res.cache_status(HitOrMiss::HIT);
                        cached_res.cache_lookup_status(HitOrMiss::HIT);
                    }
                    self.options
                        .modify_response_before_caching(&mut cached_res);
                    let res = self
                        .manager
                        .put(
                            self.options.create_cache_key(&parts, None),
                            cached_res,
                            policy,
                        )
                        .await?;
                    Ok(res)
                } else if cond_res.status == 200 {
                    let policy = match self.options.cache_options {
                        Some(options) => middleware
                            .policy_with_options(&cond_res, options)?,
                        None => middleware.policy(&cond_res)?,
                    };
                    if self.options.cache_status_headers {
                        cond_res.cache_status(HitOrMiss::MISS);
                        cond_res.cache_lookup_status(HitOrMiss::HIT);
                    }
                    // Generate metadata using the provider callback if configured
                    let response_parts = cond_res.parts()?;
                    cond_res.metadata =
                        self.options.generate_metadata(&parts, &response_parts);

                    self.options.modify_response_before_caching(&mut cond_res);
                    let res = self
                        .manager
                        .put(
                            self.options.create_cache_key(&parts, None),
                            cond_res,
                            policy,
                        )
                        .await?;
                    Ok(res)
                } else {
                    // Return fresh response for any status other than 304 or 200
                    if self.options.cache_status_headers {
                        cond_res.cache_status(HitOrMiss::MISS);
                        cond_res.cache_lookup_status(HitOrMiss::HIT);
                    }
                    Ok(cond_res)
                }
            }
            Err(e) => {
                if cached_res.must_revalidate() {
                    Err(e)
                } else {
                    //   111 Revalidation failed
                    //   MUST be included if a cache returns a stale response
                    //   because an attempt to revalidate the response failed,
                    //   due to an inability to reach the server.
                    // (https://tools.ietf.org/html/rfc2616#section-14.46)
                    cached_res.add_warning(
                        &req_url,
                        111,
                        "Revalidation failed",
                    );
                    if self.options.cache_status_headers {
                        cached_res.cache_status(HitOrMiss::HIT);
                    }
                    Ok(cached_res)
                }
            }
        }
    }
}

impl<T: StreamingCacheManager> HttpCacheStreamInterface
    for HttpStreamingCache<T>
where
    <T::Body as http_body::Body>::Data: Send,
    <T::Body as http_body::Body>::Error:
        Into<StreamingError> + Send + Sync + 'static,
{
    type Body = T::Body;

    fn analyze_request(
        &self,
        parts: &request::Parts,
        mode_override: Option<CacheMode>,
    ) -> Result<CacheAnalysis> {
        self.options.analyze_request_internal(parts, mode_override, self.mode)
    }

    async fn lookup_cached_response(
        &self,
        key: &str,
    ) -> Result<Option<(Response<Self::Body>, CachePolicy)>> {
        if let Some((mut response, policy)) = self.manager.get(key).await? {
            // Add cache status headers if enabled
            if self.options.cache_status_headers {
                response.headers_mut().insert(
                    XCACHE,
                    "HIT".parse().map_err(StreamingError::new)?,
                );
                response.headers_mut().insert(
                    XCACHELOOKUP,
                    "HIT".parse().map_err(StreamingError::new)?,
                );
            }
            Ok(Some((response, policy)))
        } else {
            Ok(None)
        }
    }

    async fn process_response<B>(
        &self,
        analysis: CacheAnalysis,
        response: Response<B>,
        metadata: Option<Vec<u8>>,
    ) -> Result<Response<Self::Body>>
    where
        B: http_body::Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
        <T::Body as http_body::Body>::Data: Send,
        <T::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static,
    {
        // For non-cacheable requests based on initial analysis, convert them to manager's body type
        if !analysis.should_cache {
            let mut converted_response =
                self.manager.convert_body(response).await?;
            // Add cache miss headers
            if self.options.cache_status_headers {
                converted_response.headers_mut().insert(
                    XCACHE,
                    "MISS".parse().map_err(StreamingError::new)?,
                );
                converted_response.headers_mut().insert(
                    XCACHELOOKUP,
                    "MISS".parse().map_err(StreamingError::new)?,
                );
            }
            return Ok(converted_response);
        }

        // Bust cache keys if needed
        for key in &analysis.cache_bust_keys {
            self.manager.delete(key).await?;
        }

        // Convert response to HttpResponse format for response-based cache mode evaluation
        let (parts, body) = response.into_parts();
        // Use provided metadata or generate from provider
        let effective_metadata = metadata.or_else(|| {
            self.options.generate_metadata(&analysis.request_parts, &parts)
        });
        let http_response = self.options.parts_to_http_response(
            &parts,
            &analysis.request_parts,
            effective_metadata.clone(),
        )?;

        // Check for response-based cache mode override
        let effective_cache_mode = self.options.evaluate_response_cache_mode(
            &analysis.request_parts,
            &http_response,
            analysis.cache_mode,
        );

        // Reconstruct response for further processing
        let response = Response::from_parts(parts, body);

        // If response-based override says NoStore, don't cache
        if effective_cache_mode == CacheMode::NoStore {
            let mut converted_response =
                self.manager.convert_body(response).await?;
            // Add cache miss headers
            if self.options.cache_status_headers {
                converted_response.headers_mut().insert(
                    XCACHE,
                    "MISS".parse().map_err(StreamingError::new)?,
                );
                converted_response.headers_mut().insert(
                    XCACHELOOKUP,
                    "MISS".parse().map_err(StreamingError::new)?,
                );
            }
            return Ok(converted_response);
        }

        // Create policy for the response
        let (parts, body) = response.into_parts();
        let policy =
            self.options.create_cache_policy(&analysis.request_parts, &parts);

        // Reconstruct response for caching
        let response = Response::from_parts(parts, body);

        let should_cache_response = self.options.should_cache_response(
            effective_cache_mode,
            &http_response,
            analysis.is_get_head,
            &policy,
        );

        if should_cache_response {
            // Extract URL from request parts for caching
            let request_url =
                extract_url_from_request_parts(&analysis.request_parts)?;

            // Cache the response using the streaming manager
            let mut cached_response = self
                .manager
                .put(
                    analysis.cache_key,
                    response,
                    policy,
                    request_url,
                    effective_metadata,
                )
                .await?;

            // Add cache miss headers (response is being stored for first time)
            if self.options.cache_status_headers {
                cached_response.headers_mut().insert(
                    XCACHE,
                    "MISS".parse().map_err(StreamingError::new)?,
                );
                cached_response.headers_mut().insert(
                    XCACHELOOKUP,
                    "MISS".parse().map_err(StreamingError::new)?,
                );
            }
            Ok(cached_response)
        } else {
            // Don't cache, just convert to manager's body type
            let mut converted_response =
                self.manager.convert_body(response).await?;
            // Add cache miss headers
            if self.options.cache_status_headers {
                converted_response.headers_mut().insert(
                    XCACHE,
                    "MISS".parse().map_err(StreamingError::new)?,
                );
                converted_response.headers_mut().insert(
                    XCACHELOOKUP,
                    "MISS".parse().map_err(StreamingError::new)?,
                );
            }
            Ok(converted_response)
        }
    }

    fn prepare_conditional_request(
        &self,
        parts: &mut request::Parts,
        _cached_response: &Response<Self::Body>,
        policy: &CachePolicy,
    ) -> Result<()> {
        let before_req = policy.before_request(parts, SystemTime::now());
        if let BeforeRequest::Stale { request, .. } = before_req {
            parts.headers.extend(request.headers);
        }
        Ok(())
    }

    async fn handle_not_modified(
        &self,
        cached_response: Response<Self::Body>,
        fresh_parts: &response::Parts,
    ) -> Result<Response<Self::Body>> {
        let (mut parts, body) = cached_response.into_parts();

        // Update headers from the 304 response
        parts.headers.extend(fresh_parts.headers.clone());

        Ok(Response::from_parts(parts, body))
    }
}

impl<T: CacheManager> HttpCacheInterface for HttpCache<T> {
    fn analyze_request(
        &self,
        parts: &request::Parts,
        mode_override: Option<CacheMode>,
    ) -> Result<CacheAnalysis> {
        self.options.analyze_request_internal(parts, mode_override, self.mode)
    }

    async fn lookup_cached_response(
        &self,
        key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        self.manager.get(key).await
    }

    async fn process_response(
        &self,
        analysis: CacheAnalysis,
        response: Response<Vec<u8>>,
        metadata: Option<Vec<u8>>,
    ) -> Result<Response<Vec<u8>>> {
        if !analysis.should_cache {
            return Ok(response);
        }

        // Bust cache keys if needed
        for key in &analysis.cache_bust_keys {
            self.manager.delete(key).await?;
        }

        // Convert response to HttpResponse format
        let (parts, body) = response.into_parts();
        // Use provided metadata or generate from provider
        let effective_metadata = metadata.or_else(|| {
            self.options.generate_metadata(&analysis.request_parts, &parts)
        });
        let mut http_response = self.options.parts_to_http_response(
            &parts,
            &analysis.request_parts,
            effective_metadata,
        )?;
        http_response.body = body.clone(); // Include the body for buffered cache managers

        // Check for response-based cache mode override
        let effective_cache_mode = self.options.evaluate_response_cache_mode(
            &analysis.request_parts,
            &http_response,
            analysis.cache_mode,
        );

        // If response-based override says NoStore, don't cache
        if effective_cache_mode == CacheMode::NoStore {
            let response = Response::from_parts(parts, body);
            return Ok(response);
        }

        // Create policy and determine if we should cache based on response-based mode
        let policy = self.options.create_cache_policy(
            &analysis.request_parts,
            &http_response.parts()?,
        );

        let should_cache_response = self.options.should_cache_response(
            effective_cache_mode,
            &http_response,
            analysis.is_get_head,
            &policy,
        );

        if should_cache_response {
            let cached_response = self
                .manager
                .put(analysis.cache_key, http_response, policy)
                .await?;

            // Convert back to standard Response
            let response_parts = cached_response.parts()?;
            let mut response = Response::builder()
                .status(response_parts.status)
                .version(response_parts.version)
                .body(cached_response.body)?;

            // Copy headers from the response parts
            *response.headers_mut() = response_parts.headers;

            Ok(response)
        } else {
            // Don't cache, return original response
            let response = Response::from_parts(parts, body);
            Ok(response)
        }
    }

    fn prepare_conditional_request(
        &self,
        parts: &mut request::Parts,
        _cached_response: &HttpResponse,
        policy: &CachePolicy,
    ) -> Result<()> {
        let before_req = policy.before_request(parts, SystemTime::now());
        if let BeforeRequest::Stale { request, .. } = before_req {
            parts.headers.extend(request.headers);
        }
        Ok(())
    }

    async fn handle_not_modified(
        &self,
        mut cached_response: HttpResponse,
        fresh_parts: &response::Parts,
    ) -> Result<HttpResponse> {
        cached_response.update_headers(fresh_parts)?;
        if self.options.cache_status_headers {
            cached_response.cache_status(HitOrMiss::HIT);
            cached_response.cache_lookup_status(HitOrMiss::HIT);
        }
        Ok(cached_response)
    }
}

#[allow(dead_code)]
#[cfg(test)]
mod test;
