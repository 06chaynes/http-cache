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
//! # http-cache-ureq
//!
//! HTTP caching wrapper for the [ureq] HTTP client.
//!
//! This crate provides a caching wrapper around the ureq HTTP client that implements
//! HTTP caching according to RFC 7234. Since ureq is a synchronous HTTP client, this
//! wrapper uses the [smol] async runtime to integrate with the async http-cache system.
//!
//! ## Features
//!
//! - `json` - Enables JSON request/response support via `send_json()` and `into_json()` methods (requires `serde_json`)
//! - `manager-cacache` - Enable [cacache](https://docs.rs/cacache/) cache manager (default)
//! - `manager-moka` - Enable [moka](https://docs.rs/moka/) cache manager
//!
//! ## Basic Usage
//!
//! ```no_run
//! use http_cache_ureq::{CachedAgent, CACacheManager, CacheMode};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     smol::block_on(async {
//!         let agent = CachedAgent::builder()
//!             .cache_manager(CACacheManager::new("./cache".into(), true))
//!             .cache_mode(CacheMode::Default)
//!             .build()?;
//!         
//!         // This request will be cached according to response headers
//!         let response = agent.get("https://httpbin.org/cache/60").call().await?;
//!         println!("Status: {}", response.status());
//!         println!("Cached: {}", response.is_cached());
//!         println!("Response: {}", response.into_string()?);
//!         
//!         // Subsequent identical requests may be served from cache
//!         let cached_response = agent.get("https://httpbin.org/cache/60").call().await?;
//!         println!("Cached status: {}", cached_response.status());
//!         println!("Is cached: {}", cached_response.is_cached());
//!         println!("Cached response: {}", cached_response.into_string()?);
//!         
//!         Ok(())
//!     })
//! }
//! ```
//!
//! ## Cache Modes
//!
//! Control caching behavior with different modes:
//!
//! ```no_run
//! use http_cache_ureq::{CachedAgent, CACacheManager, CacheMode};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     smol::block_on(async {
//!         let agent = CachedAgent::builder()
//!             .cache_manager(CACacheManager::new("./cache".into(), true))
//!             .cache_mode(CacheMode::ForceCache) // Cache everything, ignore headers
//!             .build()?;
//!         
//!         // This will be cached even if headers say not to cache
//!         let response = agent.get("https://httpbin.org/uuid").call().await?;
//!         println!("Response: {}", response.into_string()?);
//!         
//!         Ok(())
//!     })
//! }
//! ```
//!
//! ## JSON Support
//!
//! Enable the `json` feature to send and parse JSON data:
//!
//! ```no_run
//! # #[cfg(feature = "json")]
//! use http_cache_ureq::{CachedAgent, CACacheManager, CacheMode};
//! # #[cfg(feature = "json")]
//! use serde_json::json;
//!
//! # #[cfg(feature = "json")]
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     smol::block_on(async {
//!         let agent = CachedAgent::builder()
//!             .cache_manager(CACacheManager::new("./cache".into(), true))
//!             .cache_mode(CacheMode::Default)
//!             .build()?;
//!         
//!         // Send JSON data
//!         let response = agent.post("https://httpbin.org/post")
//!             .send_json(json!({"key": "value"}))
//!             .await?;
//!         
//!         // Parse JSON response
//!         let json: serde_json::Value = response.into_json()?;
//!         println!("Response: {}", json);
//!         
//!         Ok(())
//!     })
//! }
//! # #[cfg(not(feature = "json"))]
//! # fn main() {}
//! ```
//!
//! ## In-Memory Caching
//!
//! Use the Moka in-memory cache:
//!
//! ```no_run
//! # #[cfg(feature = "manager-moka")]
//! use http_cache_ureq::{CachedAgent, MokaManager, MokaCache, CacheMode};
//! # #[cfg(feature = "manager-moka")]
//!
//! # #[cfg(feature = "manager-moka")]
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     smol::block_on(async {
//!         let agent = CachedAgent::builder()
//!             .cache_manager(MokaManager::new(MokaCache::new(1000))) // Max 1000 entries
//!             .cache_mode(CacheMode::Default)
//!             .build()?;
//!             
//!         let response = agent.get("https://httpbin.org/cache/60").call().await?;
//!         println!("Response: {}", response.into_string()?);
//!         
//!         Ok(())
//!     })
//! }
//! # #[cfg(not(feature = "manager-moka"))]
//! # fn main() {}
//! ```
//!
//! ## Custom Cache Keys
//!
//! Customize how cache keys are generated:
//!
//! ```no_run
//! use http_cache_ureq::{CachedAgent, CACacheManager, CacheMode, HttpCacheOptions};
//! use std::sync::Arc;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     smol::block_on(async {
//!     let options = HttpCacheOptions {
//!         cache_key: Some(Arc::new(|parts: &http::request::Parts| {
//!             // Include query parameters in cache key
//!             format!("{}:{}", parts.method, parts.uri)
//!         })),
//!         ..Default::default()
//!     };
//!     
//!     let agent = CachedAgent::builder()
//!         .cache_manager(CACacheManager::new("./cache".into(), true))
//!         .cache_mode(CacheMode::Default)
//!         .cache_options(options)
//!         .build()?;
//!         
//!     let response = agent.get("https://httpbin.org/cache/60?param=value").call().await?;
//!     println!("Response: {}", response.into_string()?);
//!     
//!         Ok(())
//!     })
//! }
//! ```
//!
//! ## Maximum TTL Control
//!
//! Set a maximum time-to-live for cached responses, particularly useful with `CacheMode::IgnoreRules`:
//!
//! ```no_run
//! use http_cache_ureq::{CachedAgent, CACacheManager, CacheMode, HttpCacheOptions};
//! use std::time::Duration;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     smol::block_on(async {
//!         let agent = CachedAgent::builder()
//!             .cache_manager(CACacheManager::new("./cache".into(), true))
//!             .cache_mode(CacheMode::IgnoreRules) // Ignore server cache-control headers
//!             .cache_options(HttpCacheOptions {
//!                 max_ttl: Some(Duration::from_secs(300)), // Limit cache to 5 minutes regardless of server headers
//!                 ..Default::default()
//!             })
//!             .build()?;
//!         
//!         // This will be cached for max 5 minutes even if server says cache longer
//!         let response = agent.get("https://httpbin.org/cache/3600").call().await?;
//!         println!("Response: {}", response.into_string()?);
//!         
//!         Ok(())
//!     })
//! }
//! ```

// Re-export unified error types from http-cache core
pub use http_cache::{BadRequest, HttpCacheError};

use std::{
    collections::HashMap, result::Result, str::FromStr, time::SystemTime,
};

use async_trait::async_trait;

pub use http::request::Parts;
use http::{header::CACHE_CONTROL, Method};
use http_cache::{
    url_parse, BoxError, CacheManager, CacheOptions, HitOrMiss, HttpResponse,
    Middleware, Url, XCACHE, XCACHELOOKUP,
};
use http_cache_semantics::CachePolicy;

pub use http_cache::{
    CacheMode, HttpCache, HttpCacheOptions, ResponseCacheModeFn,
};

#[cfg(feature = "manager-cacache")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-cacache")))]
pub use http_cache::CACacheManager;

#[cfg(feature = "manager-moka")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-moka")))]
pub use http_cache::{MokaCache, MokaCacheBuilder, MokaManager};

#[cfg(feature = "rate-limiting")]
#[cfg_attr(docsrs, doc(cfg(feature = "rate-limiting")))]
pub use http_cache::rate_limiting::{
    CacheAwareRateLimiter, DirectRateLimiter, DomainRateLimiter, Quota,
};

/// A cached HTTP agent that wraps ureq with HTTP caching capabilities
#[derive(Debug, Clone)]
pub struct CachedAgent<T: CacheManager> {
    agent: ureq::Agent,
    cache: HttpCache<T>,
}

/// Builder for creating a CachedAgent
#[derive(Debug)]
pub struct CachedAgentBuilder<T: CacheManager> {
    agent_config: Option<ureq::config::Config>,
    cache_manager: Option<T>,
    cache_mode: CacheMode,
    cache_options: HttpCacheOptions,
}

impl<T: CacheManager> Default for CachedAgentBuilder<T> {
    fn default() -> Self {
        Self {
            agent_config: None,
            cache_manager: None,
            cache_mode: CacheMode::Default,
            cache_options: HttpCacheOptions::default(),
        }
    }
}

impl<T: CacheManager> CachedAgentBuilder<T> {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the ureq agent configuration
    ///
    /// The provided configuration will be used to preserve your settings like
    /// timeout, proxy, TLS config, and user agent. However, `http_status_as_error`
    /// will always be set to `false` to ensure proper cache operation.
    ///
    /// This is necessary because the cache middleware needs to see all HTTP responses
    /// (including 4xx and 5xx status codes) to make proper caching decisions.
    pub fn agent_config(mut self, config: ureq::config::Config) -> Self {
        self.agent_config = Some(config);
        self
    }

    /// Set the cache manager
    pub fn cache_manager(mut self, manager: T) -> Self {
        self.cache_manager = Some(manager);
        self
    }

    /// Set the cache mode
    pub fn cache_mode(mut self, mode: CacheMode) -> Self {
        self.cache_mode = mode;
        self
    }

    /// Set cache options
    pub fn cache_options(mut self, options: HttpCacheOptions) -> Self {
        self.cache_options = options;
        self
    }

    /// Build the cached agent
    pub fn build(self) -> Result<CachedAgent<T>, HttpCacheError> {
        let agent = if let Some(user_config) = self.agent_config {
            // Extract user preferences and rebuild with cache-compatible settings
            let mut config_builder =
                ureq::config::Config::builder().http_status_as_error(false); // Force this to false for cache compatibility

            // Preserve user's timeout settings
            let timeouts = user_config.timeouts();
            if timeouts.global.is_some()
                || timeouts.connect.is_some()
                || timeouts.send_request.is_some()
            {
                if let Some(global) = timeouts.global {
                    config_builder =
                        config_builder.timeout_global(Some(global));
                }
                if let Some(connect) = timeouts.connect {
                    config_builder =
                        config_builder.timeout_connect(Some(connect));
                }
                if let Some(send_request) = timeouts.send_request {
                    config_builder =
                        config_builder.timeout_send_request(Some(send_request));
                }
            }

            // Preserve user's proxy setting
            if let Some(proxy) = user_config.proxy() {
                config_builder = config_builder.proxy(Some(proxy.clone()));
            }

            // Preserve user's TLS config
            let tls_config = user_config.tls_config();
            config_builder = config_builder.tls_config(tls_config.clone());

            // Preserve user's user agent
            let user_agent = user_config.user_agent();
            config_builder = config_builder.user_agent(user_agent.clone());

            let config = config_builder.build();
            ureq::Agent::new_with_config(config)
        } else {
            // Create default config with http_status_as_error disabled
            let config = ureq::config::Config::builder()
                .http_status_as_error(false)
                .build();
            ureq::Agent::new_with_config(config)
        };

        let cache_manager = self.cache_manager.ok_or_else(|| {
            HttpCacheError::Cache("Cache manager is required".to_string())
        })?;

        Ok(CachedAgent {
            agent,
            cache: HttpCache {
                mode: self.cache_mode,
                manager: cache_manager,
                options: self.cache_options,
            },
        })
    }
}

impl<T: CacheManager> CachedAgent<T> {
    /// Create a new builder
    pub fn builder() -> CachedAgentBuilder<T> {
        CachedAgentBuilder::new()
    }

    /// Create a GET request
    pub fn get(&self, url: &str) -> CachedRequestBuilder<'_, T> {
        CachedRequestBuilder {
            agent: self,
            method: "GET".to_string(),
            url: url.to_string(),
            headers: Vec::new(),
        }
    }

    /// Create a POST request  
    pub fn post(&self, url: &str) -> CachedRequestBuilder<'_, T> {
        CachedRequestBuilder {
            agent: self,
            method: "POST".to_string(),
            url: url.to_string(),
            headers: Vec::new(),
        }
    }

    /// Create a PUT request
    pub fn put(&self, url: &str) -> CachedRequestBuilder<'_, T> {
        CachedRequestBuilder {
            agent: self,
            method: "PUT".to_string(),
            url: url.to_string(),
            headers: Vec::new(),
        }
    }

    /// Create a DELETE request
    pub fn delete(&self, url: &str) -> CachedRequestBuilder<'_, T> {
        CachedRequestBuilder {
            agent: self,
            method: "DELETE".to_string(),
            url: url.to_string(),
            headers: Vec::new(),
        }
    }

    /// Create a HEAD request
    pub fn head(&self, url: &str) -> CachedRequestBuilder<'_, T> {
        CachedRequestBuilder {
            agent: self,
            method: "HEAD".to_string(),
            url: url.to_string(),
            headers: Vec::new(),
        }
    }

    /// Create a request with a custom method
    pub fn request(
        &self,
        method: &str,
        url: &str,
    ) -> CachedRequestBuilder<'_, T> {
        CachedRequestBuilder {
            agent: self,
            method: method.to_string(),
            url: url.to_string(),
            headers: Vec::new(),
        }
    }
}

/// A cached HTTP request builder that integrates ureq requests with HTTP caching
#[derive(Debug)]
pub struct CachedRequestBuilder<'a, T: CacheManager> {
    agent: &'a CachedAgent<T>,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
}

impl<'a, T: CacheManager> CachedRequestBuilder<'a, T> {
    /// Add a header to the request
    pub fn set(mut self, header: &str, value: &str) -> Self {
        self.headers.push((header.to_string(), value.to_string()));
        self
    }

    /// Send JSON data with the request
    #[cfg(feature = "json")]
    #[cfg_attr(docsrs, doc(cfg(feature = "json")))]
    pub async fn send_json(
        self,
        data: serde_json::Value,
    ) -> Result<CachedResponse, HttpCacheError> {
        let agent = self.agent.agent.clone();
        let url = self.url.clone();
        let method = self.method;
        let headers = self.headers.clone();
        let url_for_response = url.clone();

        let response = smol::unblock(move || {
            execute_json_request(&agent, &method, &url, &headers, data).map_err(
                |e| {
                    HttpCacheError::http(Box::new(std::io::Error::other(
                        e.to_string(),
                    )))
                },
            )
        })
        .await?;

        let cached = smol::unblock(move || {
            CachedResponse::from_ureq_response(response, &url_for_response)
        })
        .await?;

        Ok(cached)
    }

    /// Send string data with the request
    pub async fn send_string(
        self,
        data: &str,
    ) -> Result<CachedResponse, HttpCacheError> {
        let data = data.to_string();
        let agent = self.agent.agent.clone();
        let url = self.url.clone();
        let method = self.method;
        let headers = self.headers.clone();
        let url_for_response = url.clone();

        let response = smol::unblock(move || {
            execute_request(&agent, &method, &url, &headers, Some(&data))
                .map_err(|e| {
                    HttpCacheError::http(Box::new(std::io::Error::other(
                        e.to_string(),
                    )))
                })
        })
        .await?;

        let cached = smol::unblock(move || {
            CachedResponse::from_ureq_response(response, &url_for_response)
        })
        .await?;

        Ok(cached)
    }

    /// Execute the request with caching
    pub async fn call(self) -> Result<CachedResponse, HttpCacheError> {
        let mut middleware = UreqMiddleware {
            method: self.method.to_string(),
            url: self.url.clone(),
            headers: self.headers.clone(),
            agent: &self.agent.agent,
        };

        // Check if we can cache this request
        if self
            .agent
            .cache
            .can_cache_request(&middleware)
            .map_err(|e| HttpCacheError::Cache(e.to_string()))?
        {
            // Use the cache system
            let response = self
                .agent
                .cache
                .run(middleware)
                .await
                .map_err(|e| HttpCacheError::Cache(e.to_string()))?;

            Ok(CachedResponse::from(response))
        } else {
            // Execute without cache but add cache headers
            self.agent
                .cache
                .run_no_cache(&mut middleware)
                .await
                .map_err(|e| HttpCacheError::Cache(e.to_string()))?;

            // Execute the request directly
            let agent = self.agent.agent.clone();
            let url = self.url.clone();
            let method = self.method;
            let headers = self.headers.clone();
            let url_for_response = url.clone();
            let cache_status_headers =
                self.agent.cache.options.cache_status_headers;

            let response = smol::unblock(move || {
                execute_request(&agent, &method, &url, &headers, None).map_err(
                    |e| {
                        HttpCacheError::http(Box::new(std::io::Error::other(
                            e.to_string(),
                        )))
                    },
                )
            })
            .await?;

            let mut cached_response = smol::unblock(move || {
                CachedResponse::from_ureq_response(response, &url_for_response)
            })
            .await?;

            // Add cache status headers if enabled
            if cache_status_headers {
                cached_response
                    .headers
                    .entry(XCACHE.to_string())
                    .or_insert_with(Vec::new)
                    .push(HitOrMiss::MISS.to_string());
                cached_response
                    .headers
                    .entry(XCACHELOOKUP.to_string())
                    .or_insert_with(Vec::new)
                    .push(HitOrMiss::MISS.to_string());
            }

            Ok(cached_response)
        }
    }
}

/// Middleware implementation for ureq integration
struct UreqMiddleware<'a> {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    agent: &'a ureq::Agent,
}

fn is_cacheable_method(method: &str) -> bool {
    matches!(method, "GET" | "HEAD")
}

/// Universal function to execute HTTP requests - replaces all method-specific duplication
fn execute_request(
    agent: &ureq::Agent,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&str>,
) -> Result<http::Response<ureq::Body>, ureq::Error> {
    // Build http::Request directly - eliminates all method-specific switching
    let mut http_request = http::Request::builder().method(method).uri(url);

    // Add headers
    for (name, value) in headers {
        http_request = http_request.header(name, value);
    }

    // Build request with or without body
    let request = match body {
        Some(data) => http_request.body(data.as_bytes().to_vec()),
        None => http_request.body(Vec::new()),
    }
    .map_err(|e| ureq::Error::BadUri(e.to_string()))?;

    // Use ureq's universal run method - this replaces ALL the method-specific logic
    agent.run(request)
}

#[cfg(feature = "json")]
/// Universal function for JSON requests - eliminates method-specific duplication
fn execute_json_request(
    agent: &ureq::Agent,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    data: serde_json::Value,
) -> Result<http::Response<ureq::Body>, ureq::Error> {
    let json_string = serde_json::to_string(&data).map_err(|e| {
        ureq::Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("JSON serialization error: {}", e),
        ))
    })?;

    // Just call the universal execute_request with JSON body
    let mut json_headers = headers.to_vec();
    json_headers
        .push(("Content-Type".to_string(), "application/json".to_string()));

    execute_request(agent, method, url, &json_headers, Some(&json_string))
}

fn convert_ureq_response_to_http_response(
    mut response: http::Response<ureq::Body>,
    url: &str,
) -> Result<HttpResponse, HttpCacheError> {
    let status = response.status();

    // Copy headers
    let headers = response.headers().into();

    // Read body as bytes to handle binary content (images, etc.)
    let body = response.body_mut().read_to_vec().map_err(|e| {
        HttpCacheError::http(Box::new(std::io::Error::other(format!(
            "Failed to read response body: {}",
            e
        ))))
    })?;

    let parsed_url = url_parse(url).map_err(|e| {
        HttpCacheError::http(Box::new(std::io::Error::other(format!(
            "Invalid URL '{}': {}",
            url, e
        ))))
    })?;

    Ok(HttpResponse {
        body,
        headers,
        status: status.as_u16(),
        url: parsed_url,
        version: http_cache::HttpVersion::Http11,
        metadata: None,
    })
}

/// A response wrapper that can represent both cached and fresh responses
#[derive(Debug)]
pub struct CachedResponse {
    status: u16,
    headers: HashMap<String, Vec<String>>,
    body: Vec<u8>,
    url: String,
    cached: bool,
}

impl CachedResponse {
    /// Get the response status code
    pub fn status(&self) -> u16 {
        self.status
    }

    /// Get the response URL
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Get a header value
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(name)
            .and_then(|values| values.first().map(|s| s.as_str()))
    }

    /// Get all header names
    pub fn headers_names(&self) -> impl Iterator<Item = &String> {
        self.headers.keys()
    }

    /// Check if this response came from cache
    pub fn is_cached(&self) -> bool {
        self.cached
    }

    /// Convert the response body to a string
    pub fn into_string(self) -> Result<String, HttpCacheError> {
        String::from_utf8(self.body).map_err(|e| {
            HttpCacheError::http(Box::new(std::io::Error::other(format!(
                "Invalid UTF-8 in response body: {}",
                e
            ))))
        })
    }

    /// Get the response body as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.body
    }

    /// Convert to bytes, consuming the response
    pub fn into_bytes(self) -> Vec<u8> {
        self.body
    }

    /// Parse response body as JSON
    #[cfg(feature = "json")]
    #[cfg_attr(docsrs, doc(cfg(feature = "json")))]
    pub fn into_json<T: serde::de::DeserializeOwned>(
        self,
    ) -> Result<T, HttpCacheError> {
        serde_json::from_slice(&self.body).map_err(|e| {
            HttpCacheError::http(Box::new(std::io::Error::other(format!(
                "JSON parse error: {}",
                e
            ))))
        })
    }
}

impl CachedResponse {
    /// Create a CachedResponse from a ureq response with a known URL
    fn from_ureq_response(
        mut response: http::Response<ureq::Body>,
        url: &str,
    ) -> Result<Self, HttpCacheError> {
        let status = response.status().as_u16();

        let mut headers = HashMap::new();
        for (name, value) in response.headers() {
            let value_str = value.to_str().unwrap_or("");
            headers
                .entry(name.as_str().to_string())
                .or_insert_with(Vec::new)
                .push(value_str.to_string());
        }

        // Note: Cache headers will be added by the cache system based on cache_status_headers option
        // Don't add them here unconditionally

        // Read the body as bytes to handle binary content (images, etc.)
        // Properly propagate errors instead of silently returning empty body
        let body = response.body_mut().read_to_vec().map_err(|e| {
            HttpCacheError::http(Box::new(std::io::Error::other(format!(
                "Failed to read response body: {}",
                e
            ))))
        })?;

        Ok(Self { status, headers, body, url: url.to_string(), cached: false })
    }
}

impl From<HttpResponse> for CachedResponse {
    fn from(response: HttpResponse) -> Self {
        // Cache headers should already be added by the cache system
        // based on the cache_status_headers option, so don't add them here
        Self {
            status: response.status,
            headers: response.headers.into(),
            body: response.body,
            url: response.url.to_string(),
            cached: true,
        }
    }
}

#[async_trait]
impl Middleware for UreqMiddleware<'_> {
    fn is_method_get_head(&self) -> bool {
        is_cacheable_method(&self.method)
    }

    fn policy(
        &self,
        response: &HttpResponse,
    ) -> http_cache::Result<CachePolicy> {
        let parts = self.build_http_parts()?;
        Ok(CachePolicy::new(&parts, &response.parts()?))
    }

    fn policy_with_options(
        &self,
        response: &HttpResponse,
        options: CacheOptions,
    ) -> http_cache::Result<CachePolicy> {
        let parts = self.build_http_parts()?;
        Ok(CachePolicy::new_options(
            &parts,
            &response.parts()?,
            SystemTime::now(),
            options,
        ))
    }

    fn update_headers(&mut self, parts: &Parts) -> http_cache::Result<()> {
        for (name, value) in parts.headers.iter() {
            let value_str = value.to_str().map_err(|e| {
                BoxError::from(format!("Invalid header value: {}", e))
            })?;
            self.headers
                .push((name.as_str().to_string(), value_str.to_string()));
        }
        Ok(())
    }

    fn force_no_cache(&mut self) -> http_cache::Result<()> {
        self.headers
            .push((CACHE_CONTROL.as_str().to_string(), "no-cache".to_string()));
        Ok(())
    }

    fn parts(&self) -> http_cache::Result<Parts> {
        self.build_http_parts()
    }

    fn url(&self) -> http_cache::Result<Url> {
        url_parse(&self.url)
    }

    fn method(&self) -> http_cache::Result<String> {
        Ok(self.method.clone())
    }

    async fn remote_fetch(&mut self) -> http_cache::Result<HttpResponse> {
        let agent = self.agent.clone();
        let method = self.method.clone();
        let url = self.url.clone();
        let headers = self.headers.clone();

        let url_for_conversion = url.clone();
        let response = smol::unblock(move || {
            execute_request(&agent, &method, &url, &headers, None)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(BoxError::from)?;

        // Convert the blocking response and read body on a blocking thread
        let http_response = smol::unblock(move || {
            convert_ureq_response_to_http_response(
                response,
                &url_for_conversion,
            )
            .map_err(|e| e.to_string())
        })
        .await
        .map_err(BoxError::from)?;

        Ok(http_response)
    }
}

impl UreqMiddleware<'_> {
    fn build_http_parts(&self) -> http_cache::Result<Parts> {
        let method = Method::from_str(&self.method)
            .map_err(|e| BoxError::from(format!("Invalid method: {}", e)))?;

        let uri = self
            .url
            .parse::<http::Uri>()
            .map_err(|e| BoxError::from(format!("Invalid URI: {}", e)))?;

        let mut http_request = http::Request::builder().method(method).uri(uri);

        // Add headers
        for (name, value) in &self.headers {
            http_request = http_request.header(name, value);
        }

        let req = http_request.body(()).map_err(|e| {
            BoxError::from(format!("Failed to build HTTP request: {}", e))
        })?;

        Ok(req.into_parts().0)
    }
}

#[cfg(test)]
mod test;
