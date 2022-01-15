//! A caching middleware that follows HTTP caching rules, thanks to
//! [`http-cache-semantics`](https://github.com/kornelski/rusty-http-cache-semantics).
//! By default, it uses [`cacache`](https://github.com/zkat/cacache-rs) as the backend cache manager.
//!
//! ## Supported Clients
//!
//! - **Surf** **Should likely be registered after any middleware modifying the request*
//! - **Reqwest** **Uses [reqwest-middleware](https://github.com/TrueLayer/reqwest-middleware) for middleware support*
//!
//! ## Examples
//!
//! ### Surf (requires feature: `client-surf`)
//!
//! ```ignore
//! use http_cache::{Cache, CacheMode, CACacheManager};
//!
//! #[async_std::main]
//! async fn main() -> surf::Result<()> {
//!     let req = surf::get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching");
//!     surf::client()
//!         .with(Cache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::default(),
//!             options: None,
//!         })
//!         .send(req)
//!         .await?;
//!     Ok(())
//! }
//! ```
//!
//! ### Reqwest (requires feature: `client-reqwest`)
//!
//! ```ignore
//! use reqwest::Client;
//! use reqwest_middleware::{ClientBuilder, Result};
//! use http_cache::{Cache, CacheMode, CACacheManager};
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::default(),
//!             options: None,
//!         })
//!         .build();
//!     client
//!         .get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
//!         .send()
//!         .await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Features
//!
//! The following features are available. By default `manager-cacache` is enabled.
//!
//! - `manager-cacache` (default): use [cacache](https://github.com/zkat/cacache-rs),
//! a high-performance disk cache, for the manager backend.
//! - `client-surf` (disabled): enables [surf](https://github.com/http-rs/surf) client support.
//! - `client-reqwest` (disabled): enables [reqwest](https://github.com/seanmonstar/reqwest) client support.
#![forbid(unsafe_code, future_incompatible)]
#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    nonstandard_style,
    unused_qualifications,
    unused_import_braces,
    unused_extern_crates,
    rustdoc::missing_doc_code_examples,
    trivial_casts,
    trivial_numeric_casts
)]
mod error;
mod managers;
mod middleware;

pub use error::CacheError;

#[cfg(feature = "manager-cacache")]
pub use managers::cacache::CACacheManager;

/// Options struct provided by
/// [`http-cache-semantics`](https://github.com/kornelski/rusty-http-cache-semantics).
pub use http_cache_semantics::CacheOptions;

use http::{header::CACHE_CONTROL, request, response, StatusCode};
use std::{collections::HashMap, str::FromStr, time::SystemTime};

use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use serde::{Deserialize, Serialize};
use url::Url;

/// A `Result` typedef to use with the [`CacheError`] type
pub type Result<T> = std::result::Result<T, CacheError>;

/// Represents an HTTP version
#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
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
}

/// Describes the functionality required for interfacing with HTTP client middleware
#[async_trait::async_trait]
pub(crate) trait Middleware {
    fn is_method_get_head(&self) -> bool;
    fn policy(&self, response: &HttpResponse) -> Result<CachePolicy>;
    fn policy_with_options(
        &self,
        response: &HttpResponse,
        options: CacheOptions,
    ) -> Result<CachePolicy>;
    fn update_headers(&mut self, parts: request::Parts) -> Result<()>;
    fn set_no_cache(&mut self) -> Result<()>;
    fn parts(&self) -> Result<request::Parts>;
    fn url(&self) -> Result<&Url>;
    fn method(&self) -> Result<String>;
    async fn remote_fetch(&mut self) -> Result<HttpResponse>;
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

/// Similar to [make-fetch-happen cache options](https://github.com/npm/make-fetch-happen#--optscache).
/// Passed in when the [`Cache`] struct is being built.
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

/// Caches requests according to http spec.
#[derive(Debug, Clone)]
pub struct Cache<T: CacheManager + Send + Sync + 'static> {
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
impl<T: CacheManager + Send + Sync + 'static> Cache<T> {
    pub(crate) async fn run(
        &self,
        mut middleware: impl Middleware,
    ) -> Result<HttpResponse> {
        let is_cacheable = middleware.is_method_get_head()
            && self.mode != CacheMode::NoStore
            && self.mode != CacheMode::Reload;
        if !is_cacheable {
            return middleware.remote_fetch().await;
        }
        if let Some(store) =
            self.manager.get(&middleware.method()?, middleware.url()?).await?
        {
            let (mut res, policy) = store;
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
                    self.conditional_fetch(middleware, res, policy).await
                }
                CacheMode::ForceCache | CacheMode::OnlyIfCached => {
                    //   112 Disconnected operation
                    // SHOULD be included if the cache is intentionally disconnected from
                    // the rest of the network for a period of time.
                    // (https://tools.ietf.org/html/rfc2616#section-14.46)
                    res.add_warning(res_url, 112, "Disconnected operation");
                    Ok(res)
                }
                _ => self.remote_fetch(&mut middleware).await,
            }
        } else {
            match self.mode {
                CacheMode::OnlyIfCached => {
                    // ENOTCACHED
                    return Ok(HttpResponse {
                        body: b"GatewayTimeout".to_vec(),
                        headers: Default::default(),
                        status: 504,
                        url: middleware.url()?.clone(),
                        version: HttpVersion::Http11,
                    });
                }
                _ => self.remote_fetch(&mut middleware).await,
            }
        }
    }

    async fn remote_fetch(
        &self,
        middleware: &mut impl Middleware,
    ) -> Result<HttpResponse> {
        let res = middleware.remote_fetch().await?;
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
                .put(&middleware.method()?, middleware.url()?, res, policy)
                .await?)
        } else if !middleware.is_method_get_head() {
            self.manager
                .delete(&middleware.method()?, middleware.url()?)
                .await?;
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
            Ok(cond_res) => {
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
                    let res = self
                        .manager
                        .put(
                            &middleware.method()?,
                            &req_url,
                            cached_res,
                            policy,
                        )
                        .await?;
                    Ok(res)
                } else {
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
                    Ok(cached_res)
                }
            }
        }
    }
}
