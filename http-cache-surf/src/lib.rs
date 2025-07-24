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
//! according to RFC 7234. It supports both traditional buffered responses and streaming
//! responses for large payloads, with various cache modes and storage backends.
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
//! ## Streaming Support
//!
//! Handle large responses without buffering them entirely in memory:
//!
//! ```no_run
//! use surf::Client;
//! use http_cache_surf::StreamingCache;  
//! use http_cache::{FileCacheManager, CacheMode};
//! use macro_rules_attribute::apply;
//! use smol_macros::main;
//!
//! #[apply(main!)]
//! async fn main() -> surf::Result<()> {
//!     let client = surf::Client::new()
//!         .with(StreamingCache::new(
//!             FileCacheManager::new("./cache".into()),
//!             CacheMode::Default,
//!         ));
//!
//!     // Stream large responses
//!     let mut res = client.get("https://httpbin.org/stream/20").await?;
//!     println!("Streaming response: {}", res.body_string().await?);
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
//! use moka::future::Cache as MokaCache;
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
use std::time::SystemTime;
use std::{collections::HashMap, str::FromStr};

use anyhow::anyhow;
use bytes::Bytes;
use http::{
    header::CACHE_CONTROL,
    request::{self, Parts},
};
use http_body::Body;
use http_body_util::BodyExt;
#[cfg(feature = "streaming")]
pub use http_cache::StreamingCacheManager;
use http_cache::{
    BadHeader, BoxError, CacheManager, CacheOptions, HitOrMiss, HttpResponse,
    HttpStreamingCache, Middleware, Result, XCACHE, XCACHELOOKUP,
};
pub use http_cache::{CacheMode, HttpCache};
use http_cache_semantics::CachePolicy;
use http_types::{
    headers::HeaderValue as HttpTypesHeaderValue,
    Response as HttpTypesResponse, StatusCode as HttpTypesStatusCode,
    Version as HttpTypesVersion,
};
use http_types::{Method as HttpTypesMethod, Request, Url};
use surf::{middleware::Next, Client};

#[cfg(feature = "streaming")]
// Re-export streaming types for convenience
pub use http_cache::StreamingBody;

// Re-export managers and cache types
#[cfg(feature = "manager-cacache")]
pub use http_cache::CACacheManager;

pub use http_cache::HttpCacheOptions;

#[cfg(feature = "manager-moka")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-moka")))]
pub use http_cache::{MokaCache, MokaCacheBuilder, MokaManager};

/// A wrapper around [`HttpCache`] that implements [`surf::middleware::Middleware`]
#[derive(Debug, Clone)]
pub struct Cache<T: CacheManager>(pub HttpCache<T>);

#[cfg(feature = "streaming")]
/// Streaming cache wrapper that implements [`surf::middleware::Middleware`] for streaming responses
#[derive(Debug, Clone)]
pub struct StreamingCache<T: StreamingCacheManager> {
    cache: HttpStreamingCache<T>,
}

#[cfg(feature = "streaming")]
impl<T: StreamingCacheManager> StreamingCache<T> {
    /// Create a new streaming cache with the given manager and mode
    pub fn new(manager: T, mode: CacheMode) -> Self {
        Self {
            cache: HttpStreamingCache {
                mode,
                manager,
                options: Default::default(),
            },
        }
    }

    /// Create a new streaming cache with custom options
    pub fn with_options(
        manager: T,
        mode: CacheMode,
        options: HttpCacheOptions,
    ) -> Self {
        Self { cache: HttpStreamingCache { mode, manager, options } }
    }
}
mod error;

pub use error::BadRequest;
#[cfg(feature = "streaming")]
pub use error::SurfStreamingError;

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
                headers.insert(
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
        let mut headers = HashMap::new();
        for header in res.iter() {
            headers.insert(
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
        })
    }
}

fn to_http_types_error(e: BoxError) -> http_types::Error {
    http_types::Error::from(anyhow!(e))
}

// Helper function to convert surf Request to http::Request for analysis
fn convert_surf_request_to_http(
    req: &Request,
) -> anyhow::Result<http::Request<()>> {
    let mut http_req = http::Request::builder()
        .method(req.method().as_ref())
        .uri(req.url().as_str());

    for header in req.iter() {
        http_req = http_req.header(header.0.as_str(), header.1.as_str());
    }

    Ok(http_req.body(())?)
}

// Helper function to convert surf Response to http::Response with Full body
async fn convert_surf_response_to_http_full_body(
    mut response: surf::Response,
) -> anyhow::Result<http::Response<http_body_util::Full<Bytes>>> {
    let status = response.status();
    let version = response.version().unwrap_or(HttpTypesVersion::Http1_1);
    let body_bytes = response
        .body_bytes()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read body: {}", e))?;

    let mut http_response = http::Response::builder()
        .status(u16::from(status))
        .version(match version {
            HttpTypesVersion::Http1_0 => http::Version::HTTP_10,
            HttpTypesVersion::Http1_1 => http::Version::HTTP_11,
            // HttpTypesVersion::H2 not available in this version of http-types
            _ => http::Version::HTTP_11, // Default fallback
        });

    for header in response.iter() {
        http_response =
            http_response.header(header.0.as_str(), header.1.as_str());
    }

    Ok(http_response
        .body(http_body_util::Full::new(Bytes::from(body_bytes)))?)
}

// Helper function to convert streaming response back to surf Response by buffering it
async fn convert_streaming_response_to_surf<B>(
    response: http::Response<B>,
) -> anyhow::Result<surf::Response>
where
    B: Body + Send + 'static,
{
    let (parts, body) = response.into_parts();

    // Collect the streaming body into bytes, handling errors generically
    let body_bytes = match BodyExt::collect(body).await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => {
            return Err(anyhow::anyhow!("Failed to collect streaming body"))
        }
    };

    let mut surf_response = HttpTypesResponse::new(
        HttpTypesStatusCode::try_from(parts.status.as_u16())
            .map_err(|e| anyhow::anyhow!("Invalid status code: {}", e))?,
    );

    // Set headers
    for (name, value) in parts.headers.iter() {
        let surf_value =
            HttpTypesHeaderValue::from_bytes(value.as_bytes().to_vec())
                .map_err(|e| anyhow::anyhow!("Invalid header value: {}", e))?;
        surf_response.insert_header(name.as_str(), surf_value);
    }

    // Set version and body
    let surf_version = match parts.version {
        http::Version::HTTP_10 => HttpTypesVersion::Http1_0,
        http::Version::HTTP_11 => HttpTypesVersion::Http1_1,
        // http::Version::HTTP_2 => HttpTypesVersion::H2, // H2 not available
        _ => HttpTypesVersion::Http1_1, // Default fallback
    };
    surf_response.set_version(Some(surf_version));
    surf_response.set_body(body_bytes.to_vec()); // Convert Bytes to Vec<u8>

    Ok(surf::Response::from(surf_response))
}

// Helper function to convert surf response parts for 304 handling
fn convert_surf_response_to_http_parts(
    response: surf::Response,
) -> anyhow::Result<(http::response::Parts, ())> {
    let status = response.status();
    let version = response.version().unwrap_or(HttpTypesVersion::Http1_1);

    let mut http_response = http::Response::builder()
        .status(u16::from(status))
        .version(match version {
            HttpTypesVersion::Http1_0 => http::Version::HTTP_10,
            HttpTypesVersion::Http1_1 => http::Version::HTTP_11,
            // HttpTypesVersion::H2 not available in this version of http-types
            _ => http::Version::HTTP_11, // Default fallback
        });

    for header in response.iter() {
        http_response =
            http_response.header(header.0.as_str(), header.1.as_str());
    }

    let response = http_response.body(())?;
    Ok(response.into_parts())
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
        if self
            .0
            .can_cache_request(&middleware)
            .map_err(|e| http_types::Error::from(anyhow!(e)))?
        {
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

#[cfg(feature = "streaming")]
#[surf::utils::async_trait]
impl<T: StreamingCacheManager> surf::middleware::Middleware
    for StreamingCache<T>
where
    T::Body: From<Bytes> + Send + 'static,
    <T::Body as Body>::Data: Send,
    <T::Body as Body>::Error:
        Into<http_cache::StreamingError> + Send + Sync + 'static,
{
    async fn handle(
        &self,
        req: surf::Request,
        client: Client,
        next: Next<'_>,
    ) -> surf::Result<surf::Response> {
        use http_cache::HttpCacheStreamInterface;

        let req: Request = req.into();

        // Convert to http::Request for analysis
        let http_request = convert_surf_request_to_http(&req)
            .map_err(|e| http_types::Error::from(anyhow!(e)))?;

        // Analyze the request
        let analysis = self
            .cache
            .analyze_request(&http_request.into_parts().0, None)
            .map_err(|e| http_types::Error::from(anyhow!(e)))?;

        // Check if we should bypass cache entirely
        if !analysis.should_cache {
            return next.run(req.into(), client).await;
        }

        // Look up cached response
        if let Some((cached_response, policy)) = self
            .cache
            .lookup_cached_response(&analysis.cache_key)
            .await
            .map_err(|e| http_types::Error::from(anyhow!(e)))?
        {
            // Check if cached response is still fresh
            use http_cache_semantics::BeforeRequest;
            let before_req = policy
                .before_request(&analysis.request_parts, SystemTime::now());

            match before_req {
                BeforeRequest::Fresh(_fresh_parts) => {
                    // Return cached response directly
                    return convert_streaming_response_to_surf(cached_response)
                        .await
                        .map_err(|e| http_types::Error::from(anyhow!(e)));
                }
                BeforeRequest::Stale {
                    request: conditional_request, ..
                } => {
                    // Create conditional request
                    let mut conditional_surf_req = req.clone();
                    for (name, value) in conditional_request.headers.iter() {
                        let surf_value = HttpTypesHeaderValue::from_str(
                            value.to_str().unwrap_or(""),
                        )
                        .map_err(|e| http_types::Error::from(anyhow!(e)))?;
                        conditional_surf_req
                            .insert_header(name.as_str(), surf_value);
                    }

                    let conditional_response = next
                        .run(conditional_surf_req.into(), client.clone())
                        .await?;

                    if conditional_response.status() == 304u16 {
                        // Handle not modified - return the cached response updated with fresh headers
                        let (fresh_parts, _) =
                            convert_surf_response_to_http_parts(
                                conditional_response,
                            )
                            .map_err(|e| http_types::Error::from(anyhow!(e)))?;
                        let updated_response = self
                            .cache
                            .handle_not_modified(cached_response, &fresh_parts)
                            .await
                            .map_err(|e| http_types::Error::from(anyhow!(e)))?;

                        return convert_streaming_response_to_surf(
                            updated_response,
                        )
                        .await
                        .map_err(|e| http_types::Error::from(anyhow!(e)));
                    } else {
                        // Fresh response received, process it through the cache
                        let http_response =
                            convert_surf_response_to_http_full_body(
                                conditional_response,
                            )
                            .await
                            .map_err(|e| http_types::Error::from(anyhow!(e)))?;
                        let cached_response = self
                            .cache
                            .process_response(analysis, http_response)
                            .await
                            .map_err(|e| http_types::Error::from(anyhow!(e)))?;

                        return convert_streaming_response_to_surf(
                            cached_response,
                        )
                        .await
                        .map_err(|e| http_types::Error::from(anyhow!(e)));
                    }
                }
            }
        }

        // Fetch fresh response from upstream (cache miss or no cached response)
        let response = next.run(req.into(), client).await?;
        let http_response = convert_surf_response_to_http_full_body(response)
            .await
            .map_err(|e| http_types::Error::from(anyhow!(e)))?;

        // Process and potentially cache the response
        let cached_response = self
            .cache
            .process_response(analysis, http_response)
            .await
            .map_err(|e| http_types::Error::from(anyhow!(e)))?;

        convert_streaming_response_to_surf(cached_response)
            .await
            .map_err(|e| http_types::Error::from(anyhow!(e)))
    }
}

#[cfg(test)]
mod test;
