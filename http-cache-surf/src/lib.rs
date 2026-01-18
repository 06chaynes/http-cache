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
//! HTTP caching middleware for the surf HTTP client.
//!
//! This crate provides middleware for the surf HTTP client that implements HTTP caching
//! according to RFC 7234. It supports various cache modes and storage backends.
//!
//! ## Basic Usage
//!
//! Add HTTP caching to your surf client:
//!
//! ```no_run
//! use surf::Client;
//! use http_cache_surf::{Cache, CACacheManager, HttpCache, CacheMode};
//! use macro_rules_attribute::apply;
//! use smol_macros::main;
//!
//! #[apply(main!)]
//! async fn main() -> surf::Result<()> {
//!     let client = surf::Client::new()
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::new("./cache".into(), true),
//!             options: Default::default(),
//!         }));
//!
//!     // This request will be cached according to response headers
//!     let mut res = client.get("https://httpbin.org/cache/60").await?;
//!     println!("Response: {}", res.body_string().await?);
//!     
//!     // Subsequent identical requests may be served from cache
//!     let mut cached_res = client.get("https://httpbin.org/cache/60").await?;
//!     println!("Cached response: {}", cached_res.body_string().await?);
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Cache Modes
//!
//! Control caching behavior with different modes:
//!
//! ```no_run
//! use surf::Client;
//! use http_cache_surf::{Cache, CACacheManager, HttpCache, CacheMode};
//! use macro_rules_attribute::apply;
//! use smol_macros::main;
//!
//! #[apply(main!)]
//! async fn main() -> surf::Result<()> {
//!     let client = surf::Client::new()
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::ForceCache, // Cache everything, ignore headers
//!             manager: CACacheManager::new("./cache".into(), true),
//!             options: Default::default(),
//!         }));
//!
//!     // This will be cached even if headers say not to cache
//!     let mut res = client.get("https://httpbin.org/uuid").await?;
//!     println!("{}", res.body_string().await?);
//!     Ok(())
//! }
//! ```
//!
//! ## In-Memory Caching
//!
//! Use the Moka in-memory cache:
//!
//! ```no_run
//! # #[cfg(feature = "manager-moka")]
//! use surf::Client;
//! # #[cfg(feature = "manager-moka")]
//! use http_cache_surf::{Cache, MokaManager, HttpCache, CacheMode};
//! # #[cfg(feature = "manager-moka")]
//! use http_cache_surf::MokaCache;
//! # #[cfg(feature = "manager-moka")]
//! use macro_rules_attribute::apply;
//! # #[cfg(feature = "manager-moka")]
//! use smol_macros::main;
//!
//! # #[cfg(feature = "manager-moka")]
//! #[apply(main!)]
//! async fn main() -> surf::Result<()> {
//!     let client = surf::Client::new()
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: MokaManager::new(MokaCache::new(1000)), // Max 1000 entries
//!             options: Default::default(),
//!         }));
//!
//!     let mut res = client.get("https://httpbin.org/cache/60").await?;
//!     println!("{}", res.body_string().await?);
//!     Ok(())
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
//! use surf::Client;
//! use http_cache_surf::{Cache, CACacheManager, HttpCache, CacheMode};
//! use http_cache::HttpCacheOptions;
//! use std::sync::Arc;
//! use macro_rules_attribute::apply;
//! use smol_macros::main;
//!
//! #[apply(main!)]
//! async fn main() -> surf::Result<()> {
//!     let options = HttpCacheOptions {
//!         cache_key: Some(Arc::new(|parts: &http::request::Parts| {
//!             // Include query parameters in cache key
//!             format!("{}:{}", parts.method, parts.uri)
//!         })),
//!         ..Default::default()
//!     };
//!     
//!     let client = surf::Client::new()
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::new("./cache".into(), true),
//!             options,
//!         }));
//!
//!     let mut res = client.get("https://httpbin.org/cache/60?param=value").await?;
//!     println!("{}", res.body_string().await?);
//!     Ok(())
//! }
//! ```

use std::convert::TryInto;
use std::str::FromStr;
use std::time::SystemTime;

use http::{
    header::CACHE_CONTROL,
    request::{self, Parts},
};
use http_cache::{
    BadHeader, BoxError, CacheManager, CacheOptions, HitOrMiss, HttpResponse,
    Middleware, Result, XCACHE, XCACHELOOKUP,
};
pub use http_cache::{CacheMode, HttpCache, HttpHeaders};
use http_cache_semantics::CachePolicy;
use http_types::{
    headers::HeaderValue as HttpTypesHeaderValue,
    Response as HttpTypesResponse, StatusCode as HttpTypesStatusCode,
    Version as HttpTypesVersion,
};
use http_types::{Method as HttpTypesMethod, Request, Url};
use surf::{middleware::Next, Client};

// Re-export managers and cache types
#[cfg(feature = "manager-cacache")]
pub use http_cache::CACacheManager;

pub use http_cache::HttpCacheOptions;
pub use http_cache::ResponseCacheModeFn;

#[cfg(feature = "manager-moka")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-moka")))]
pub use http_cache::{MokaCache, MokaCacheBuilder, MokaManager};

#[cfg(feature = "rate-limiting")]
#[cfg_attr(docsrs, doc(cfg(feature = "rate-limiting")))]
pub use http_cache::rate_limiting::{
    CacheAwareRateLimiter, DirectRateLimiter, DomainRateLimiter, Quota,
};

/// A wrapper around [`HttpCache`] that implements [`surf::middleware::Middleware`]
#[derive(Debug, Clone)]
pub struct Cache<T: CacheManager>(pub HttpCache<T>);

// Re-export unified error types from http-cache core
pub use http_cache::{BadRequest, HttpCacheError};

/// Implements ['Middleware'] for surf
pub(crate) struct SurfMiddleware<'a> {
    pub req: Request,
    pub client: Client,
    pub next: Next<'a>,
}

#[async_trait::async_trait]
impl Middleware for SurfMiddleware<'_> {
    fn is_method_get_head(&self) -> bool {
        self.req.method() == HttpTypesMethod::Get
            || self.req.method() == HttpTypesMethod::Head
    }
    fn policy(&self, response: &HttpResponse) -> Result<CachePolicy> {
        Ok(CachePolicy::new(&self.parts()?, &response.parts()?))
    }
    fn policy_with_options(
        &self,
        response: &HttpResponse,
        options: CacheOptions,
    ) -> Result<CachePolicy> {
        Ok(CachePolicy::new_options(
            &self.parts()?,
            &response.parts()?,
            SystemTime::now(),
            options,
        ))
    }
    fn update_headers(&mut self, parts: &Parts) -> Result<()> {
        for header in parts.headers.iter() {
            let value = match HttpTypesHeaderValue::from_str(header.1.to_str()?)
            {
                Ok(v) => v,
                Err(_e) => return Err(Box::new(BadHeader)),
            };
            self.req.insert_header(header.0.as_str(), value);
        }
        Ok(())
    }
    fn force_no_cache(&mut self) -> Result<()> {
        self.req.insert_header(CACHE_CONTROL.as_str(), "no-cache");
        Ok(())
    }
    fn parts(&self) -> Result<Parts> {
        let mut converted = request::Builder::new()
            .method(self.req.method().as_ref())
            .uri(self.req.url().as_str())
            .body(())?;
        {
            let headers = converted.headers_mut();
            for header in self.req.iter() {
                headers.append(
                    http::header::HeaderName::from_str(header.0.as_str())?,
                    http::HeaderValue::from_str(header.1.as_str())?,
                );
            }
        }
        Ok(converted.into_parts().0)
    }
    fn url(&self) -> Result<Url> {
        Ok(self.req.url().clone())
    }
    fn method(&self) -> Result<String> {
        Ok(self.req.method().as_ref().to_string())
    }
    async fn remote_fetch(&mut self) -> Result<HttpResponse> {
        let url = self.req.url().clone();
        let mut res =
            self.next.run(self.req.clone().into(), self.client.clone()).await?;
        let mut headers = HttpHeaders::new();
        for header in res.iter() {
            headers.append(
                header.0.as_str().to_owned(),
                header.1.as_str().to_owned(),
            );
        }
        let status = res.status().into();
        let version = res.version().unwrap_or(HttpTypesVersion::Http1_1);
        let body: Vec<u8> = res.body_bytes().await?;
        Ok(HttpResponse {
            body,
            headers,
            status,
            url,
            version: version.try_into()?,
            metadata: None,
        })
    }
}

fn to_http_types_error(e: BoxError) -> http_types::Error {
    http_types::Error::from_str(500, format!("HTTP cache error: {e}"))
}

#[surf::utils::async_trait]
impl<T: CacheManager> surf::middleware::Middleware for Cache<T> {
    async fn handle(
        &self,
        req: surf::Request,
        client: Client,
        next: Next<'_>,
    ) -> std::result::Result<surf::Response, http_types::Error> {
        let req: Request = req.into();
        let mut middleware = SurfMiddleware { req, client, next };
        if self.0.can_cache_request(&middleware).map_err(to_http_types_error)? {
            let res =
                self.0.run(middleware).await.map_err(to_http_types_error)?;
            let mut converted = HttpTypesResponse::new(HttpTypesStatusCode::Ok);
            for header in &res.headers {
                let val = HttpTypesHeaderValue::from_bytes(
                    header.1.as_bytes().to_vec(),
                )?;
                converted.insert_header(header.0.as_str(), val);
            }
            converted.set_status(res.status.try_into()?);
            converted.set_version(Some(res.version.into()));
            converted.set_body(res.body);
            Ok(surf::Response::from(converted))
        } else {
            self.0
                .run_no_cache(&mut middleware)
                .await
                .map_err(to_http_types_error)?;
            let mut res = middleware
                .next
                .run(middleware.req.into(), middleware.client)
                .await?;
            let miss = HitOrMiss::MISS.to_string();
            res.append_header(XCACHE, miss.clone());
            res.append_header(XCACHELOOKUP, miss);
            Ok(res)
        }
    }
}

#[cfg(test)]
mod test;
