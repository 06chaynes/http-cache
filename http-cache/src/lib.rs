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
#![cfg_attr(docsrs, feature(doc_cfg))]
//! A caching middleware that follows HTTP caching rules, thanks to
//! [`http-cache-semantics`](https://github.com/kornelski/rusty-http-cache-semantics).
//! By default, it uses [`cacache`](https://github.com/zkat/cacache-rs) as the backend cache manager.
//!
//! ## Features
//!
//! The following features are available. By default `manager-cacache` and `cacache-async-std` are enabled.
//!
//! - `manager-cacache` (default): enable [cacache](https://github.com/zkat/cacache-rs),
//! a high-performance disk cache, backend manager.
//! - `cacache-async-std` (default): enable [async-std](https://github.com/async-rs/async-std) runtime support for cacache.
//! - `cacache-tokio` (disabled): enable [tokio](https://github.com/tokio-rs/tokio) runtime support for cacache.
//! - `manager-moka` (disabled): enable [moka](https://github.com/moka-rs/moka),
//! a high-performance in-memory cache, backend manager.
//! - `with-http-types` (disabled): enable [http-types](https://github.com/http-rs/http-types)
//! type conversion support
mod error;
mod managers;

use std::{
    collections::HashMap,
    convert::TryFrom,
    fmt::{self, Debug},
    str::FromStr,
    sync::Arc,
    time::SystemTime,
};

use http::{header::CACHE_CONTROL, request, response, StatusCode};
use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use serde::{Deserialize, Serialize};
use url::Url;

pub use error::{BadHeader, BadVersion, BoxError, Result};

#[cfg(feature = "manager-cacache")]
pub use managers::cacache::CACacheManager;

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
        self.headers.get(CACHE_CONTROL.as_str()).map_or(false, |val| {
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

/// Describes the functionality required for interfacing with HTTP client middleware
#[async_trait::async_trait]
pub trait Middleware: Send {
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

/// Similar to [make-fetch-happen cache options](https://github.com/npm/make-fetch-happen#--optscache).
/// Passed in when the [`HttpCache`] struct is being built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    /// Will inspect the HTTP cache on the way to the network.
    /// If there is a fresh response it will be used.
    /// If there is a stale response a conditional request will be created,
    /// and a normal request otherwise.
    /// It then updates the HTTP cache with the response.
    /// If the revalidation request fails (for example, on a 500 or if you're offline),
    /// the stale response will be returned.
    Default,
    /// Behaves as if there is no HTTP cache at all.
    NoStore,
    /// Behaves as if there is no HTTP cache on the way to the network.
    /// Ergo, it creates a normal request and updates the HTTP cache with the response.
    Reload,
    /// Creates a conditional request if there is a response in the HTTP cache
    /// and a normal request otherwise. It then updates the HTTP cache with the response.
    NoCache,
    /// Uses any response in the HTTP cache matching the request,
    /// not paying attention to staleness. If there was no response,
    /// it creates a normal request and updates the HTTP cache with the response.
    ForceCache,
    /// Uses any response in the HTTP cache matching the request,
    /// not paying attention to staleness. If there was no response,
    /// it returns a network error.
    OnlyIfCached,
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

/// Can be used to override the default [`CacheOptions`] and cache key.
/// The cache key is a closure that takes [`http::request::Parts`] and returns a [`String`].
#[derive(Default, Clone)]
pub struct HttpCacheOptions {
    /// Override the default cache options.
    pub cache_options: Option<CacheOptions>,
    /// Override the default cache key generator.
    pub cache_key: Option<CacheKey>,
}

impl Debug for HttpCacheOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpCacheOptions")
            .field("cache_options", &self.cache_options)
            .field("cache_key", &"Fn(&request::Parts) -> String")
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

#[allow(dead_code)]
impl<T: CacheManager> HttpCache<T> {
    /// Attempts to run the passed middleware along with the cache
    pub async fn run(
        &self,
        mut middleware: impl Middleware,
    ) -> Result<HttpResponse> {
        let is_cacheable = middleware.is_method_get_head()
            && self.mode != CacheMode::NoStore
            && self.mode != CacheMode::Reload;
        if !is_cacheable {
            return self.remote_fetch(&mut middleware).await;
        }
        if let Some(store) = self
            .manager
            .get(&self.options.create_cache_key(&middleware.parts()?, None))
            .await?
        {
            let (mut res, policy) = store;
            res.cache_lookup_status(HitOrMiss::HIT);
            if let Some(warning_code) = res.warning_code() {
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
                    res.remove_warning();
                }
            }

            match self.mode {
                CacheMode::Default => {
                    self.conditional_fetch(middleware, res, policy).await
                }
                CacheMode::NoCache => {
                    middleware.force_no_cache()?;
                    let mut res = self.remote_fetch(&mut middleware).await?;
                    res.cache_lookup_status(HitOrMiss::HIT);
                    Ok(res)
                }
                CacheMode::ForceCache | CacheMode::OnlyIfCached => {
                    //   112 Disconnected operation
                    // SHOULD be included if the cache is intentionally disconnected from
                    // the rest of the network for a period of time.
                    // (https://tools.ietf.org/html/rfc2616#section-14.46)
                    res.add_warning(
                        &res.url.clone(),
                        112,
                        "Disconnected operation",
                    );
                    res.cache_status(HitOrMiss::HIT);
                    Ok(res)
                }
                _ => self.remote_fetch(&mut middleware).await,
            }
        } else {
            match self.mode {
                CacheMode::OnlyIfCached => {
                    // ENOTCACHED
                    let mut res = HttpResponse {
                        body: b"GatewayTimeout".to_vec(),
                        headers: HashMap::default(),
                        status: 504,
                        url: middleware.url()?,
                        version: HttpVersion::Http11,
                    };
                    res.cache_status(HitOrMiss::MISS);
                    res.cache_lookup_status(HitOrMiss::MISS);
                    Ok(res)
                }
                _ => self.remote_fetch(&mut middleware).await,
            }
        }
    }

    async fn remote_fetch(
        &self,
        middleware: &mut impl Middleware,
    ) -> Result<HttpResponse> {
        let mut res = middleware.remote_fetch().await?;
        res.cache_status(HitOrMiss::MISS);
        res.cache_lookup_status(HitOrMiss::MISS);
        let policy = match self.options.cache_options {
            Some(options) => middleware.policy_with_options(&res, options)?,
            None => middleware.policy(&res)?,
        };
        let is_get_head = middleware.is_method_get_head();
        let is_cacheable = is_get_head
            && self.mode != CacheMode::NoStore
            && self.mode != CacheMode::Reload
            && res.status == 200
            && policy.is_storable();
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
                cached_res.cache_status(HitOrMiss::HIT);
                cached_res.cache_lookup_status(HitOrMiss::HIT);
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
                    cached_res.cache_status(HitOrMiss::HIT);
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
                    cached_res.cache_status(HitOrMiss::HIT);
                    cached_res.cache_lookup_status(HitOrMiss::HIT);
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
                    cond_res.cache_status(HitOrMiss::MISS);
                    cond_res.cache_lookup_status(HitOrMiss::HIT);
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
                    cached_res.cache_status(HitOrMiss::HIT);
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
                    cached_res.cache_status(HitOrMiss::HIT);
                    Ok(cached_res)
                }
            }
        }
    }
}

#[cfg(test)]
mod test;
