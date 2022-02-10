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
//! The following features are available. By default `manager-cacache` is enabled.
//!
//! - `manager-cacache` (default): enable [cacache](https://github.com/zkat/cacache-rs),
//! a high-performance disk cache, backend manager.
//! - `manager-moka` (disabled): enable [moka](https://github.com/moka-rs/moka),
//! a high-performance in-memory cache, backend manager.
//! - `with-http-types` (disabled): enable [http-types](https://github.com/http-rs/http-types)
//! type conversion support
mod error;
mod managers;

use std::{
    collections::HashMap, convert::TryFrom, fmt, str::FromStr, time::SystemTime,
};

use http::{header::CACHE_CONTROL, request, response, StatusCode};
use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use serde::{Deserialize, Serialize};
use url::Url;

pub use error::{CacheError, Result};

#[cfg(feature = "manager-cacache")]
pub use managers::cacache::CACacheManager;

#[cfg(feature = "manager-moka")]
pub use managers::moka::MokaManager;

// Exposing the moka cache for convenience, renaming to avoid naming conflicts
#[cfg(feature = "manager-moka")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-moka")))]
pub use moka::future::{Cache as MokaCache, CacheBuilder as MokaCacheBuilder};

// Custom headers used to indicate cache status (hit or miss)
/// `x-cache` header: Did this proxy serve the result from cache (HIT for yes, MISS for no)
pub const XCACHE: &str = "x-cache";
/// `x-cache-lookup` header: Did the proxy have a cacheable response to the request (HIT for yes, MISS for no)
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
            HitOrMiss::HIT => write!(f, "HIT"),
            HitOrMiss::MISS => write!(f, "MISS"),
        }
    }
}

/// Represents an HTTP version
#[derive(Debug, Copy, Clone, PartialEq, Deserialize, Serialize)]
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
    /// Returns http::response::Parts
    pub fn parts(&self) -> Result<response::Parts> {
        let mut converted =
            response::Builder::new().status(self.status).body(())?;
        {
            let headers = converted.headers_mut();
            for header in self.headers.iter() {
                headers.insert(
                    http::header::HeaderName::from_str(header.0.as_str())?,
                    http::HeaderValue::from_str(header.1.as_str())?,
                );
            }
        }
        Ok(converted.into_parts().0)
    }

    /// Returns the status code of the warning header if present
    pub fn warning_code(&self) -> Option<usize> {
        self.headers.get("Warning").and_then(|hdr| {
            hdr.as_str().chars().take(3).collect::<String>().parse().ok()
        })
    }

    /// Adds a warning header to a response
    pub fn add_warning(&mut self, url: Url, code: usize, message: &str) {
        // Warning    = "Warning" ":" 1#warning-value
        // warning-value = warn-code SP warn-agent SP warn-text [SP warn-date]
        // warn-code  = 3DIGIT
        // warn-agent = ( host [ ":" port ] ) | pseudonym
        //                 ; the name or pseudonym of the server adding
        //                 ; the Warning header, for use in debugging
        // warn-text  = quoted-string
        // warn-date  = <"> HTTP-date <">
        // (https://tools.ietf.org/html/rfc2616#section-14.46)
        self.headers.insert(
            "Warning".to_string(),
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
        self.headers.remove("Warning");
    }

    /// Update the headers from http::response::Parts
    pub fn update_headers(&mut self, parts: response::Parts) -> Result<()> {
        for header in parts.headers.iter() {
            self.headers.insert(
                header.0.as_str().to_string(),
                header.1.to_str()?.to_string(),
            );
        }
        Ok(())
    }

    /// Checks if the Cache-Control header contains the must-revalidate directive
    pub fn must_revalidate(&self) -> bool {
        if let Some(val) = self.headers.get(CACHE_CONTROL.as_str()) {
            val.as_str().to_lowercase().contains("must-revalidate")
        } else {
            false
        }
    }

    /// Adds the custom `x-cache` header to the response
    pub fn set_cache_status_header(
        &mut self,
        cache_status: HitOrMiss,
    ) -> Result<()> {
        self.headers.insert(XCACHE.to_string(), cache_status.to_string());
        Ok(())
    }

    /// Adds the custom `x-cache-lookup` header to the response
    pub fn set_cache_lookup_status_header(
        &mut self,
        lookup_status: HitOrMiss,
    ) -> Result<()> {
        self.headers
            .insert(XCACHELOOKUP.to_string(), lookup_status.to_string());
        Ok(())
    }
}

/// A trait providing methods for storing, reading, and removing cache records.
#[async_trait::async_trait]
pub trait CacheManager {
    /// Attempts to pull a cached response and related policy from cache.
    async fn get(
        &self,
        method: &str,
        url: &Url,
    ) -> Result<Option<(HttpResponse, CachePolicy)>>;
    /// Attempts to cache a response and related policy.
    async fn put(
        &self,
        method: &str,
        url: &Url,
        res: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse>;
    /// Attempts to remove a record from cache.
    async fn delete(&self, method: &str, url: &Url) -> Result<()>;
}

/// Describes the functionality required for interfacing with HTTP client middleware
#[async_trait::async_trait]
pub trait Middleware {
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
    fn update_headers(&mut self, parts: request::Parts) -> Result<()>;
    /// Attempts to force the "no-cache" directive on the request
    fn set_no_cache(&mut self) -> Result<()>;
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
    type Error = CacheError;

    fn try_from(value: http::Version) -> Result<Self> {
        Ok(match value {
            http::Version::HTTP_09 => HttpVersion::Http09,
            http::Version::HTTP_10 => HttpVersion::Http10,
            http::Version::HTTP_11 => HttpVersion::Http11,
            http::Version::HTTP_2 => HttpVersion::H2,
            http::Version::HTTP_3 => HttpVersion::H3,
            _ => return Err(CacheError::BadVersion),
        })
    }
}

impl From<HttpVersion> for http::Version {
    fn from(value: HttpVersion) -> Self {
        match value {
            HttpVersion::Http09 => http::Version::HTTP_09,
            HttpVersion::Http10 => http::Version::HTTP_10,
            HttpVersion::Http11 => http::Version::HTTP_11,
            HttpVersion::H2 => http::Version::HTTP_2,
            HttpVersion::H3 => http::Version::HTTP_3,
        }
    }
}

#[cfg(feature = "http-types")]
impl TryFrom<http_types::Version> for HttpVersion {
    type Error = CacheError;

    fn try_from(value: http_types::Version) -> Result<Self> {
        Ok(match value {
            http_types::Version::Http0_9 => HttpVersion::Http09,
            http_types::Version::Http1_0 => HttpVersion::Http10,
            http_types::Version::Http1_1 => HttpVersion::Http11,
            http_types::Version::Http2_0 => HttpVersion::H2,
            http_types::Version::Http3_0 => HttpVersion::H3,
            _ => return Err(CacheError::BadVersion),
        })
    }
}

#[cfg(feature = "http-types")]
impl From<HttpVersion> for http_types::Version {
    fn from(value: HttpVersion) -> Self {
        match value {
            HttpVersion::Http09 => http_types::Version::Http0_9,
            HttpVersion::Http10 => http_types::Version::Http1_0,
            HttpVersion::Http11 => http_types::Version::Http1_1,
            HttpVersion::H2 => http_types::Version::Http2_0,
            HttpVersion::H3 => http_types::Version::Http3_0,
        }
    }
}

/// Options struct provided by
/// [`http-cache-semantics`](https://github.com/kornelski/rusty-http-cache-semantics).
pub use http_cache_semantics::CacheOptions;

/// Caches requests according to http spec.
#[derive(Debug, Clone)]
pub struct HttpCache<T: CacheManager + Send + Sync + 'static> {
    /// Determines the manager behavior.
    pub mode: CacheMode,
    /// Manager instance that implements the [`CacheManager`] trait.
    /// By default, a manager implementation with [`cacache`](https://github.com/zkat/cacache-rs)
    /// as the backend has been provided, see [`CACacheManager`].
    pub manager: T,
    /// Override the default cache options.
    pub options: Option<CacheOptions>,
}

#[allow(dead_code)]
impl<T: CacheManager + Send + Sync + 'static> HttpCache<T> {
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
            .get(&middleware.method()?.to_uppercase(), &middleware.url()?)
            .await?
        {
            let (mut res, policy) = store;
            res.set_cache_lookup_status_header(HitOrMiss::HIT)?;
            let res_url = res.url.clone();
            if let Some(warning_code) = res.warning_code() {
                // https://tools.ietf.org/html/rfc7234#section-4.3.4
                //
                // If a stored response is selected for update, the cache MUST:
                //
                // * delete any Warning header fields in the stored response with
                //   warn-code 1xx (see Section 5.5);
                //
                // * retain any Warning header fields in the stored response with
                //   warn-code 2xx;
                //
                #[allow(clippy::manual_range_contains)]
                if warning_code >= 100 && warning_code < 200 {
                    res.remove_warning();
                }
            }

            match self.mode {
                CacheMode::Default => {
                    self.conditional_fetch(middleware, res, policy).await
                }
                CacheMode::NoCache => {
                    middleware.set_no_cache()?;
                    let mut res = self.remote_fetch(&mut middleware).await?;
                    res.set_cache_lookup_status_header(HitOrMiss::HIT)?;
                    Ok(res)
                }
                CacheMode::ForceCache | CacheMode::OnlyIfCached => {
                    //   112 Disconnected operation
                    // SHOULD be included if the cache is intentionally disconnected from
                    // the rest of the network for a period of time.
                    // (https://tools.ietf.org/html/rfc2616#section-14.46)
                    res.add_warning(res_url, 112, "Disconnected operation");
                    res.set_cache_status_header(HitOrMiss::HIT)?;
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
                        headers: Default::default(),
                        status: 504,
                        url: middleware.url()?,
                        version: HttpVersion::Http11,
                    };
                    res.set_cache_status_header(HitOrMiss::MISS)?;
                    res.set_cache_lookup_status_header(HitOrMiss::MISS)?;
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
        res.set_cache_status_header(HitOrMiss::MISS)?;
        res.set_cache_lookup_status_header(HitOrMiss::MISS)?;
        let policy = match self.options {
            Some(options) => middleware.policy_with_options(&res, options)?,
            None => middleware.policy(&res)?,
        };
        let is_cacheable = middleware.is_method_get_head()
            && self.mode != CacheMode::NoStore
            && self.mode != CacheMode::Reload
            && res.status == 200
            && policy.is_storable();
        if is_cacheable {
            Ok(self
                .manager
                .put(
                    &middleware.method()?.to_uppercase(),
                    &middleware.url()?,
                    res,
                    policy,
                )
                .await?)
        } else if !middleware.is_method_get_head() {
            match self.manager.delete("GET", &middleware.url()?).await {
                Ok(()) => {}
                Err(_e) => {}
            }
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
                cached_res.update_headers(parts)?;
                cached_res.set_cache_status_header(HitOrMiss::HIT)?;
                cached_res.set_cache_lookup_status_header(HitOrMiss::HIT)?;
                return Ok(cached_res);
            }
            BeforeRequest::Stale { request: parts, matches } => {
                if matches {
                    middleware.update_headers(parts)?;
                }
            }
        }
        let req_url = middleware.url()?.clone();
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
                        req_url.clone(),
                        111,
                        "Revalidation failed",
                    );
                    cached_res.set_cache_status_header(HitOrMiss::HIT)?;
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
                            cached_res.update_headers(parts)?;
                        }
                    }
                    cached_res.set_cache_status_header(HitOrMiss::HIT)?;
                    cached_res
                        .set_cache_lookup_status_header(HitOrMiss::HIT)?;
                    let res = self
                        .manager
                        .put(
                            &middleware.method()?.to_uppercase(),
                            &req_url,
                            cached_res,
                            policy,
                        )
                        .await?;
                    Ok(res)
                } else if cond_res.status == 200 {
                    let policy = match self.options {
                        Some(options) => middleware
                            .policy_with_options(&cond_res, options)?,
                        None => middleware.policy(&cond_res)?,
                    };
                    cond_res.set_cache_status_header(HitOrMiss::MISS)?;
                    cond_res.set_cache_lookup_status_header(HitOrMiss::HIT)?;
                    let res = self
                        .manager
                        .put(
                            &middleware.method()?.to_uppercase(),
                            &req_url,
                            cond_res,
                            policy,
                        )
                        .await?;
                    Ok(res)
                } else {
                    cached_res.set_cache_status_header(HitOrMiss::HIT)?;
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
                    cached_res.add_warning(req_url, 111, "Revalidation failed");
                    cached_res.set_cache_status_header(HitOrMiss::HIT)?;
                    Ok(cached_res)
                }
            }
        }
    }
}
