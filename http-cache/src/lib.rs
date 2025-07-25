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
//! ```
//!
//! ## Cache Modes
//!
//! Different cache modes provide different behaviors:
//!
//! ```rust
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
//! ```
//!
//! ## Custom Cache Keys
//!
//! You can customize how cache keys are generated:
//!
//! ```rust
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
//! ```
//!
//! ## Response-Based Cache Mode Override
//!
//! Override cache behavior based on the response you receive. This is useful for scenarios like
//! forcing cache for successful responses even when headers say not to cache, or never caching
//! error responses like rate limits:
//!
//! ```rust
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
//! ```
//!
//! ## Streaming Support
//!
//! For handling large responses without full buffering, use the `FileCacheManager`:
//!
//! ```rust
//! # #[cfg(feature = "streaming")]
//! # {
//! use http_cache::{StreamingBody, HttpStreamingCache, FileCacheManager};
//! use bytes::Bytes;
//! use std::path::PathBuf;
//! use http_body::Body;
//! use http_body_util::Full;
//!
//! // Create a file-based streaming cache manager
//! let manager = FileCacheManager::new(PathBuf::from("./streaming-cache"));
//!
//! // StreamingBody can handle both buffered and streaming scenarios
//! let body: StreamingBody<Full<Bytes>> = StreamingBody::buffered(Bytes::from("cached content"));
//! println!("Body size: {:?}", body.size_hint());
//! # }
//! ```
//!
//! **Note**: True streaming support requires the `FileCacheManager` with the `streaming` feature.
//! Other cache managers (CACacheManager, MokaManager, QuickManager) do not support true streaming
//! and will buffer response bodies in memory.
//!
//! ## Features
//!
//! The following features are available. By default `manager-cacache` and `cacache-smol` are enabled.
//!
//! - `manager-cacache` (default): enable [cacache](https://github.com/zkat/cacache-rs),
//! a disk cache, backend manager.
//! - `cacache-smol` (default): enable [smol](https://github.com/smol-rs/smol) runtime support for cacache.
//! - `cacache-tokio` (disabled): enable [tokio](https://github.com/tokio-rs/tokio) runtime support for cacache.
//! - `manager-moka` (disabled): enable [moka](https://github.com/moka-rs/moka),
//! an in-memory cache, backend manager.
//! - `streaming` (disabled): enable the `FileCacheManager` for true streaming cache support.
//! - `streaming-tokio` (disabled): enable streaming with tokio runtime support.
//! - `streaming-smol` (disabled): enable streaming with smol runtime support.
//! - `with-http-types` (disabled): enable [http-types](https://github.com/http-rs/http-types)
//! type conversion support
//!
//! **Note**: Only `FileCacheManager` (via the `streaming` feature) provides true streaming support.
//! Other managers will buffer response bodies in memory even when used with `StreamingCacheManager`.
//!
//! ## Integration
//!
//! This crate is designed to be used as a foundation for HTTP client and server middleware.
//! See the companion crates for specific integrations:
//!
//! - [`http-cache-reqwest`](https://docs.rs/http-cache-reqwest) for reqwest client middleware
//! - [`http-cache-surf`](https://docs.rs/http-cache-surf) for surf client middleware  
//! - [`http-cache-tower`](https://docs.rs/http-cache-tower) for tower/axum service middleware
mod body;
mod error;
mod managers;

#[cfg(feature = "streaming")]
mod runtime;

use std::{
    collections::HashMap,
    convert::TryFrom,
    fmt::{self, Debug},
    str::FromStr,
    sync::Arc,
    time::SystemTime,
};

use http::{header::CACHE_CONTROL, request, response, Response, StatusCode};
use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use serde::{Deserialize, Serialize};
use url::Url;

pub use body::StreamingBody;
pub use error::{BadHeader, BadVersion, BoxError, Result, StreamingError};

#[cfg(feature = "manager-cacache")]
pub use managers::cacache::CACacheManager;

#[cfg(feature = "streaming")]
pub use managers::streaming_cache::FileCacheManager;

#[cfg(feature = "manager-moka")]
pub use managers::moka::MokaManager;

// Exposing the moka cache for convenience, renaming to avoid naming conflicts
#[cfg(feature = "manager-moka")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-moka")))]
pub use moka::future::{Cache as MokaCache, CacheBuilder as MokaCacheBuilder};

// Custom headers used to indicate cache status (hit or miss)
/// `x-cache` header: Value will be HIT if the response was served from cache, MISS if not
pub const XCACHE: &str = "x-cache";
/// `x-cache-lookup` header: Value will be HIT if a response existed in cache, MISS if not
pub const XCACHELOOKUP: &str = "x-cache-lookup";

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
pub fn extract_url_from_request_parts(parts: &request::Parts) -> Result<Url> {
    // First check if the URI is already absolute
    if let Some(_scheme) = parts.uri.scheme() {
        // URI is absolute, use it directly
        return Url::parse(&parts.uri.to_string())
            .map_err(|_| BadHeader.into());
    }

    // Get the scheme - default to https for security, but check for explicit http
    let scheme = if let Some(host) = parts.headers.get("host") {
        let host_str = host.to_str().map_err(|_| BadHeader)?;
        // Check if this looks like a local development host
        if host_str.starts_with("localhost")
            || host_str.starts_with("127.0.0.1")
        {
            "http"
        } else if let Some(forwarded_proto) =
            parts.headers.get("x-forwarded-proto")
        {
            forwarded_proto.to_str().map_err(|_| BadHeader)?
        } else {
            "https" // Default to secure
        }
    } else {
        "https" // Default to secure if no host header
    };

    // Get the host
    let host = parts
        .headers
        .get("host")
        .ok_or(BadHeader)?
        .to_str()
        .map_err(|_| BadHeader)?;

    // Construct the full URL
    let url_string = format!(
        "{}://{}{}",
        scheme,
        host,
        parts.uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/")
    );

    Url::parse(&url_string).map_err(|_| BadHeader.into())
}

/// A basic generic type that represents an HTTP response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpResponse {
    /// HTTP response body
    pub body: Vec<u8>,
    /// HTTP response headers
    pub headers: HashMap<String, String>,
    /// HTTP response status code
    pub status: u16,
    /// HTTP response url
    pub url: Url,
    /// HTTP response version
    pub version: HttpVersion,
}

impl HttpResponse {
    /// Returns `http::response::Parts`
    pub fn parts(&self) -> Result<response::Parts> {
        let mut converted =
            response::Builder::new().status(self.status).body(())?;
        {
            let headers = converted.headers_mut();
            for header in &self.headers {
                headers.insert(
                    http::header::HeaderName::from_str(header.0.as_str())?,
                    http::HeaderValue::from_str(header.1.as_str())?,
                );
            }
        }
        Ok(converted.into_parts().0)
    }

    /// Returns the status code of the warning header if present
    #[must_use]
    pub fn warning_code(&self) -> Option<usize> {
        self.headers.get("warning").and_then(|hdr| {
            hdr.as_str().chars().take(3).collect::<String>().parse().ok()
        })
    }

    /// Adds a warning header to a response
    pub fn add_warning(&mut self, url: &Url, code: usize, message: &str) {
        // warning    = "warning" ":" 1#warning-value
        // warning-value = warn-code SP warn-agent SP warn-text [SP warn-date]
        // warn-code  = 3DIGIT
        // warn-agent = ( host [ ":" port ] ) | pseudonym
        //                 ; the name or pseudonym of the server adding
        //                 ; the warning header, for use in debugging
        // warn-text  = quoted-string
        // warn-date  = <"> HTTP-date <">
        // (https://tools.ietf.org/html/rfc2616#section-14.46)
        self.headers.insert(
            "warning".to_string(),
            format!(
                "{} {} {:?} \"{}\"",
                code,
                url.host().expect("Invalid URL"),
                message,
                httpdate::fmt_http_date(SystemTime::now())
            ),
        );
    }

    /// Removes a warning header from a response
    pub fn remove_warning(&mut self) {
        self.headers.remove("warning");
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
    pub fn must_revalidate(&self) -> bool {
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
    /// Determines if the cache status headers should be added to the response.
    pub cache_status_headers: bool,
}

impl Default for HttpCacheOptions {
    fn default() -> Self {
        Self {
            cache_options: None,
            cache_key: None,
            cache_mode_fn: None,
            response_cache_mode_fn: None,
            cache_bust: None,
            cache_status_headers: true,
        }
    }
}

impl Debug for HttpCacheOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpCacheOptions")
            .field("cache_options", &self.cache_options)
            .field("cache_key", &"Fn(&request::Parts) -> String")
            .field("cache_mode_fn", &"Fn(&request::Parts) -> CacheMode")
            .field(
                "response_cache_mode_fn",
                &"Fn(&request::Parts, &HttpResponse) -> Option<CacheMode>",
            )
            .field("cache_bust", &"Fn(&request::Parts) -> Vec<String>")
            .field("cache_status_headers", &self.cache_status_headers)
            .finish()
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

    /// Converts http::HeaderMap to HashMap<String, String> for HttpResponse
    pub fn headers_to_hashmap(
        headers: &http::HeaderMap,
    ) -> HashMap<String, String> {
        headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect()
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
            if let (Ok(header_name), Ok(header_value)) = (
                name.parse::<http::HeaderName>(),
                value.parse::<http::HeaderValue>(),
            ) {
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
    ) -> Result<HttpResponse> {
        Ok(HttpResponse {
            body: vec![], // We don't need the full body for cache mode decision
            headers: parts
                .headers
                .iter()
                .map(|(k, v)| {
                    (k.to_string(), v.to_str().unwrap_or("").to_string())
                })
                .collect(),
            status: parts.status.as_u16(),
            url: extract_url_from_request_parts(request_parts)?,
            version: parts.version.try_into()?,
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

    /// Creates a cache policy for the given request and response
    fn create_cache_policy(
        &self,
        request_parts: &request::Parts,
        response_parts: &response::Parts,
    ) -> CachePolicy {
        match self.cache_options {
            Some(options) => CachePolicy::new_options(
                request_parts,
                response_parts,
                SystemTime::now(),
                options,
            ),
            None => CachePolicy::new(request_parts, response_parts),
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

    /// Runs the actions to preform when the client middleware is running without the cache
    pub async fn run_no_cache(
        &self,
        middleware: &mut impl Middleware,
    ) -> Result<()> {
        self.manager
            .delete(
                &self
                    .options
                    .create_cache_key(&middleware.parts()?, Some("GET")),
            )
            .await
            .ok();

        let cache_key =
            self.options.create_cache_key(&middleware.parts()?, None);

        if let Some(cache_bust) = &self.options.cache_bust {
            for key_to_cache_bust in cache_bust(
                &middleware.parts()?,
                &self.options.cache_key,
                &cache_key,
            ) {
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
                        headers: HashMap::default(),
                        status: 504,
                        url: middleware.url()?,
                        version: HttpVersion::Http11,
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

        // Allow response-based cache mode override
        if let Some(response_cache_mode_fn) =
            &self.options.response_cache_mode_fn
        {
            if let Some(override_mode) =
                response_cache_mode_fn(&middleware.parts()?, &res)
            {
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
            Ok(self
                .manager
                .put(
                    self.options.create_cache_key(&middleware.parts()?, None),
                    res,
                    policy,
                )
                .await?)
        } else if !is_get_head {
            self.manager
                .delete(
                    &self
                        .options
                        .create_cache_key(&middleware.parts()?, Some("GET")),
                )
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
        let before_req =
            policy.before_request(&middleware.parts()?, SystemTime::now());
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
                        &middleware.parts()?,
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
                    let res = self
                        .manager
                        .put(
                            self.options
                                .create_cache_key(&middleware.parts()?, None),
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
                    let res = self
                        .manager
                        .put(
                            self.options
                                .create_cache_key(&middleware.parts()?, None),
                            cond_res,
                            policy,
                        )
                        .await?;
                    Ok(res)
                } else {
                    if self.options.cache_status_headers {
                        cached_res.cache_status(HitOrMiss::HIT);
                    }
                    Ok(cached_res)
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
        self.manager.get(key).await
    }

    async fn process_response<B>(
        &self,
        analysis: CacheAnalysis,
        response: Response<B>,
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
            return self.manager.convert_body(response).await;
        }

        // Bust cache keys if needed
        for key in &analysis.cache_bust_keys {
            self.manager.delete(key).await?;
        }

        // Convert response to HttpResponse format for response-based cache mode evaluation
        let (parts, body) = response.into_parts();
        let http_response = self
            .options
            .parts_to_http_response(&parts, &analysis.request_parts)?;

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
            return self.manager.convert_body(response).await;
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
            self.manager
                .put(analysis.cache_key, response, policy, request_url)
                .await
        } else {
            // Don't cache, just convert to manager's body type
            self.manager.convert_body(response).await
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
        let mut http_response = self
            .options
            .parts_to_http_response(&parts, &analysis.request_parts)?;
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
