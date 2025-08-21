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

mod error;

pub use error::{BadRequest, UreqError};

use std::{
    collections::HashMap, result::Result, str::FromStr, time::SystemTime,
};

use async_trait::async_trait;

pub use http::request::Parts;
use http::{header::CACHE_CONTROL, Method};
use http_cache::{
    BoxError, CacheManager, CacheOptions, HitOrMiss, HttpResponse, Middleware,
    XCACHE, XCACHELOOKUP,
};
use http_cache_semantics::CachePolicy;
use url::Url;

pub use http_cache::{
    CacheMode, HttpCache, HttpCacheOptions, ResponseCacheModeFn,
};

#[cfg(feature = "manager-cacache")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-cacache")))]
pub use http_cache::CACacheManager;

#[cfg(feature = "manager-moka")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-moka")))]
pub use http_cache::{MokaCache, MokaCacheBuilder, MokaManager};

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
    pub fn build(self) -> Result<CachedAgent<T>, UreqError> {
        let agent = if let Some(config) = self.agent_config {
            // We can't modify an existing Config, so we have to use it as-is
            // TODO: In the future, we might want to expose http_status_as_error as a builder option
            ureq::Agent::new_with_config(config)
        } else {
            // Create default config with http_status_as_error disabled
            let config = ureq::config::Config::builder()
                .http_status_as_error(false)
                .build();
            ureq::Agent::new_with_config(config)
        };

        let cache_manager = self.cache_manager.ok_or_else(|| {
            UreqError::Cache("Cache manager is required".to_string())
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
    pub async fn send_json(
        self,
        data: serde_json::Value,
    ) -> Result<CachedResponse, UreqError> {
        let agent = self.agent.agent.clone();
        let url = self.url.clone();
        let method = self.method;
        let headers = self.headers.clone();
        let url_for_response = url.clone();

        let response = smol::unblock(move || {
            execute_json_request(&agent, &method, &url, &headers, data)
                .map_err(|e| UreqError::Http(e.to_string()))
        })
        .await?;

        let cached = smol::unblock(move || {
            Ok::<_, UreqError>(CachedResponse::from_ureq_response(
                response,
                &url_for_response,
            ))
        })
        .await?;

        Ok(cached)
    }

    /// Send string data with the request
    pub async fn send_string(
        self,
        data: &str,
    ) -> Result<CachedResponse, UreqError> {
        let data = data.to_string();
        let agent = self.agent.agent.clone();
        let url = self.url.clone();
        let method = self.method;
        let headers = self.headers.clone();
        let url_for_response = url.clone();

        let response = smol::unblock(move || {
            execute_request(&agent, &method, &url, &headers, Some(&data))
                .map_err(|e| UreqError::Http(e.to_string()))
        })
        .await?;

        let cached = smol::unblock(move || {
            Ok::<_, UreqError>(CachedResponse::from_ureq_response(
                response,
                &url_for_response,
            ))
        })
        .await?;

        Ok(cached)
    }

    /// Execute the request with caching
    pub async fn call(self) -> Result<CachedResponse, UreqError> {
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
            .map_err(|e| UreqError::Cache(e.to_string()))?
        {
            // Use the cache system
            let response = self
                .agent
                .cache
                .run(middleware)
                .await
                .map_err(|e| UreqError::Cache(e.to_string()))?;

            Ok(CachedResponse::from(response))
        } else {
            // Execute without cache but add cache headers
            self.agent
                .cache
                .run_no_cache(&mut middleware)
                .await
                .map_err(|e| UreqError::Cache(e.to_string()))?;

            // Execute the request directly
            let agent = self.agent.agent.clone();
            let url = self.url.clone();
            let method = self.method;
            let headers = self.headers.clone();
            let url_for_response = url.clone();
            let cache_status_headers =
                self.agent.cache.options.cache_status_headers;

            let response = smol::unblock(move || {
                execute_request(&agent, &method, &url, &headers, None)
                    .map_err(|e| UreqError::Http(e.to_string()))
            })
            .await?;

            let mut cached_response = smol::unblock(move || {
                Ok::<_, UreqError>(CachedResponse::from_ureq_response(
                    response,
                    &url_for_response,
                ))
            })
            .await?;

            // Add cache status headers if enabled
            if cache_status_headers {
                cached_response
                    .headers
                    .insert(XCACHE.to_string(), HitOrMiss::MISS.to_string());
                cached_response.headers.insert(
                    XCACHELOOKUP.to_string(),
                    HitOrMiss::MISS.to_string(),
                );
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

/// Helper function to execute HTTP requests, eliminating code duplication
fn execute_request(
    agent: &ureq::Agent,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&str>,
) -> Result<http::Response<ureq::Body>, ureq::Error> {
    // Handle methods that support or require bodies differently due to ureq's type system
    match method {
        "GET" => {
            let mut req = agent.get(url);
            for (name, value) in headers {
                req = req.header(name, value);
            }
            req.call()
        }
        "HEAD" => {
            let mut req = agent.head(url);
            for (name, value) in headers {
                req = req.header(name, value);
            }
            req.call()
        }
        "DELETE" => {
            let mut req = agent.delete(url);
            for (name, value) in headers {
                req = req.header(name, value);
            }
            req.call()
        }
        "OPTIONS" => {
            let mut req = agent.options(url);
            for (name, value) in headers {
                req = req.header(name, value);
            }
            req.call()
        }
        "POST" => {
            let mut req = agent.post(url);
            for (name, value) in headers {
                req = req.header(name, value);
            }
            if let Some(body_content) = body {
                req.send(body_content)
            } else {
                req.send("")
            }
        }
        "PUT" => {
            let mut req = agent.put(url);
            for (name, value) in headers {
                req = req.header(name, value);
            }
            if let Some(body_content) = body {
                req.send(body_content)
            } else {
                req.send("")
            }
        }
        "PATCH" => {
            let mut req = agent.patch(url);
            for (name, value) in headers {
                req = req.header(name, value);
            }
            if let Some(body_content) = body {
                req.send(body_content)
            } else {
                req.send("")
            }
        }
        _ => Err(ureq::Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Unsupported method: {}", method),
        ))),
    }
}

/// Helper for JSON requests
fn execute_json_request(
    agent: &ureq::Agent,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    data: serde_json::Value,
) -> Result<http::Response<ureq::Body>, ureq::Error> {
    match method {
        "POST" => {
            let mut req = agent.post(url);
            for (name, value) in headers {
                req = req.header(name, value);
            }
            req.send_json(data)
        }
        "PUT" => {
            let mut req = agent.put(url);
            for (name, value) in headers {
                req = req.header(name, value);
            }
            req.send_json(data)
        }
        "PATCH" => {
            let mut req = agent.patch(url);
            for (name, value) in headers {
                req = req.header(name, value);
            }
            req.send_json(data)
        }
        _ => Err(ureq::Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Method {} does not support JSON body", method),
        ))),
    }
}

fn convert_ureq_response_to_http_response(
    mut response: http::Response<ureq::Body>,
    url: &str,
) -> Result<HttpResponse, UreqError> {
    let status = response.status();
    let mut headers = HashMap::new();

    // Copy headers
    for (name, value) in response.headers() {
        let value_str = value.to_str().map_err(|e| {
            UreqError::Http(format!("Invalid header value: {}", e))
        })?;
        headers.insert(name.as_str().to_string(), value_str.to_string());
    }

    // Read body using read_to_string
    let body_string = response.body_mut().read_to_string().map_err(|e| {
        UreqError::Http(format!("Failed to read response body: {}", e))
    })?;

    let body = body_string.into_bytes();

    // Parse the provided URL
    let parsed_url = Url::parse(url).map_err(|e| {
        UreqError::Http(format!("Invalid URL '{}': {}", url, e))
    })?;

    Ok(HttpResponse {
        body,
        headers,
        status: status.as_u16(),
        url: parsed_url,
        version: http_cache::HttpVersion::Http11,
    })
}

/// A response wrapper that can represent both cached and fresh responses
#[derive(Debug)]
pub struct CachedResponse {
    status: u16,
    headers: HashMap<String, String>,
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
        self.headers.get(name).map(|s| s.as_str())
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
    pub fn into_string(self) -> Result<String, UreqError> {
        String::from_utf8(self.body).map_err(|e| {
            UreqError::Http(format!("Invalid UTF-8 in response body: {}", e))
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
    pub fn into_json<T: serde::de::DeserializeOwned>(
        self,
    ) -> Result<T, UreqError> {
        serde_json::from_slice(&self.body)
            .map_err(|e| UreqError::Http(format!("JSON parse error: {}", e)))
    }
}

impl CachedResponse {
    /// Create a CachedResponse from a ureq response with a known URL
    fn from_ureq_response(
        mut response: http::Response<ureq::Body>,
        url: &str,
    ) -> Self {
        let status = response.status().as_u16();

        let mut headers = HashMap::new();
        for (name, value) in response.headers() {
            let value_str = value.to_str().unwrap_or("");
            headers.insert(name.as_str().to_string(), value_str.to_string());
        }

        // Note: Cache headers will be added by the cache system based on cache_status_headers option
        // Don't add them here unconditionally

        // Read the body
        let body = if let Ok(body_string) = response.body_mut().read_to_string()
        {
            body_string.into_bytes()
        } else {
            Vec::new()
        };

        Self { status, headers, body, url: url.to_string(), cached: false }
    }
}

impl From<HttpResponse> for CachedResponse {
    fn from(response: HttpResponse) -> Self {
        // Cache headers should already be added by the cache system
        // based on the cache_status_headers option, so don't add them here
        Self {
            status: response.status,
            headers: response.headers,
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
        Url::parse(&self.url).map_err(BoxError::from)
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
