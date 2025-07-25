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
//! # http-cache-reqwest
//!
//! HTTP caching middleware for the [reqwest] HTTP client.
//!
//! This middleware implements HTTP caching according to RFC 7234 for the reqwest HTTP client library.
//! It works as part of the [reqwest-middleware] ecosystem to provide caching capabilities.
//!
//! ```no_run
//! use reqwest::Client;
//! use reqwest_middleware::{ClientBuilder, Result};
//! use http_cache_reqwest::{Cache, CacheMode, CACacheManager, HttpCache, HttpCacheOptions};
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::new("./cache".into(), true),
//!             options: HttpCacheOptions::default(),
//!         }))
//!         .build();
//!     
//!     // This request will be cached according to response headers
//!     let response = client
//!         .get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
//!         .send()
//!         .await?;
//!     println!("Status: {}", response.status());
//!     
//!     // Subsequent identical requests may be served from cache
//!     let cached_response = client
//!         .get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
//!         .send()
//!         .await?;
//!     println!("Cached status: {}", cached_response.status());
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
//! use reqwest::Client;
//! use reqwest_middleware::ClientBuilder;
//! use http_cache_reqwest::{StreamingCache, CACacheManager, CacheMode};
//!
//! #[tokio::main]
//! async fn main() -> reqwest_middleware::Result<()> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(StreamingCache::new(
//!             CACacheManager::new("./cache".into(), true),
//!             CacheMode::Default,
//!         ))
//!         .build();
//!         
//!     // Stream large responses
//!     let response = client
//!         .get("https://httpbin.org/stream/20")
//!         .send()
//!         .await?;
//!     println!("Status: {}", response.status());
//!     
//!     // Process the streaming body
//!     let body = response.text().await?;
//!     println!("Body length: {}", body.len());
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
//! use reqwest::Client;
//! use reqwest_middleware::ClientBuilder;
//! use http_cache_reqwest::{Cache, CacheMode, CACacheManager, HttpCache, HttpCacheOptions};
//!
//! #[tokio::main]
//! async fn main() -> reqwest_middleware::Result<()> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::ForceCache, // Cache everything, ignore headers
//!             manager: CACacheManager::new("./cache".into(), true),
//!             options: HttpCacheOptions::default(),
//!         }))
//!         .build();
//!     
//!     // This will be cached even if headers say not to cache
//!     client.get("https://httpbin.org/uuid").send().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Per-Request Cache Control
//!
//! Override the cache mode on individual requests:
//!
//! ```no_run
//! use reqwest::Client;
//! use reqwest_middleware::ClientBuilder;
//! use http_cache_reqwest::{Cache, CacheMode, CACacheManager, HttpCache, HttpCacheOptions};
//!
//! #[tokio::main]
//! async fn main() -> reqwest_middleware::Result<()> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::new("./cache".into(), true),
//!             options: HttpCacheOptions::default(),
//!         }))
//!         .build();
//!     
//!     // Override cache mode for this specific request
//!     let response = client.get("https://httpbin.org/uuid")
//!         .with_extension(CacheMode::OnlyIfCached) // Only serve from cache
//!         .send()
//!         .await?;
//!         
//!     // This request bypasses cache completely
//!     let fresh_response = client.get("https://httpbin.org/uuid")
//!         .with_extension(CacheMode::NoStore)
//!         .send()
//!         .await?;
//!         
//!     Ok(())
//! }
//! ```
//!
//! ## Custom Cache Keys
//!
//! Customize how cache keys are generated:
//!
//! ```no_run
//! use reqwest::Client;
//! use reqwest_middleware::ClientBuilder;
//! use http_cache_reqwest::{Cache, CacheMode, CACacheManager, HttpCache, HttpCacheOptions};
//!
//! #[tokio::main]
//! async fn main() -> reqwest_middleware::Result<()> {
//!     let options = HttpCacheOptions {
//!         cache_key: Some(Box::new(|req| {
//!             // Include query parameters in cache key
//!             format!("{}:{}", req.method(), req.uri())
//!         })),
//!         ..Default::default()
//!     };
//!     
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::new("./cache".into(), true),
//!             options,
//!         }))
//!         .build();
//!         
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
//! use reqwest::Client;
//! # #[cfg(feature = "manager-moka")]
//! use reqwest_middleware::ClientBuilder;
//! # #[cfg(feature = "manager-moka")]
//! use http_cache_reqwest::{Cache, CacheMode, MokaManager, HttpCache, HttpCacheOptions};
//! # #[cfg(feature = "manager-moka")]
//! use moka::future::Cache as MokaCache;
//!
//! # #[cfg(feature = "manager-moka")]
//! #[tokio::main]
//! async fn main() -> reqwest_middleware::Result<()> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: MokaManager::new(MokaCache::new(1000)), // Max 1000 entries
//!             options: HttpCacheOptions::default(),
//!         }))
//!         .build();
//!         
//!     Ok(())
//! }
//! # #[cfg(not(feature = "manager-moka"))]
//! # fn main() {}
//! ```
mod error;

pub use error::BadRequest;
#[cfg(feature = "streaming")]
pub use error::ReqwestStreamingError;

use anyhow::anyhow;

use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    str::FromStr,
    time::SystemTime,
};

pub use http::request::Parts;
use http::{
    header::{HeaderName, CACHE_CONTROL},
    Extensions, HeaderValue, Method,
};
use http_cache::{
    BoxError, HitOrMiss, Middleware, Result, XCACHE, XCACHELOOKUP,
};
use http_cache_semantics::CachePolicy;
use reqwest::{Request, Response, ResponseBuilderExt};
use reqwest_middleware::{Error, Next};
use url::Url;

pub use http_cache::{
    CacheManager, CacheMode, CacheOptions, HttpCache, HttpCacheOptions,
    HttpResponse, ResponseCacheModeFn,
};

#[cfg(feature = "streaming")]
// Re-export streaming types for future use
pub use http_cache::{
    HttpCacheStreamInterface, HttpStreamingCache, StreamingBody,
    StreamingCacheManager,
};

#[cfg(feature = "manager-cacache")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-cacache")))]
pub use http_cache::CACacheManager;

#[cfg(feature = "manager-moka")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-moka")))]
pub use http_cache::{MokaCache, MokaCacheBuilder, MokaManager};

/// Wrapper for [`HttpCache`]
#[derive(Debug)]
pub struct Cache<T: CacheManager>(pub HttpCache<T>);

#[cfg(feature = "streaming")]
/// Streaming cache wrapper that implements reqwest middleware for streaming responses
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
                options: HttpCacheOptions::default(),
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

/// Implements ['Middleware'] for reqwest
pub(crate) struct ReqwestMiddleware<'a> {
    pub req: Request,
    pub next: Next<'a>,
    pub extensions: &'a mut Extensions,
}

fn clone_req(request: &Request) -> std::result::Result<Request, Error> {
    match request.try_clone() {
        Some(r) => Ok(r),
        None => Err(Error::Middleware(anyhow!(BadRequest))),
    }
}

#[async_trait::async_trait]
impl Middleware for ReqwestMiddleware<'_> {
    fn overridden_cache_mode(&self) -> Option<CacheMode> {
        self.extensions.get().cloned()
    }
    fn is_method_get_head(&self) -> bool {
        self.req.method() == Method::GET || self.req.method() == Method::HEAD
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
            self.req.headers_mut().insert(header.0.clone(), header.1.clone());
        }
        Ok(())
    }
    fn force_no_cache(&mut self) -> Result<()> {
        self.req
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_str("no-cache")?);
        Ok(())
    }
    fn parts(&self) -> Result<Parts> {
        let copied_req = clone_req(&self.req)?;
        let converted = match http::Request::try_from(copied_req) {
            Ok(r) => r,
            Err(e) => return Err(Box::new(e)),
        };
        Ok(converted.into_parts().0)
    }
    fn url(&self) -> Result<Url> {
        Ok(self.req.url().clone())
    }
    fn method(&self) -> Result<String> {
        Ok(self.req.method().as_ref().to_string())
    }
    async fn remote_fetch(&mut self) -> Result<HttpResponse> {
        let copied_req = clone_req(&self.req)?;
        let res = match self.next.clone().run(copied_req, self.extensions).await
        {
            Ok(r) => r,
            Err(e) => return Err(Box::new(e)),
        };
        let mut headers = HashMap::new();
        for header in res.headers() {
            headers.insert(
                header.0.as_str().to_owned(),
                header.1.to_str()?.to_owned(),
            );
        }
        let url = res.url().clone();
        let status = res.status().into();
        let version = res.version();
        let body: Vec<u8> = match res.bytes().await {
            Ok(b) => b,
            Err(e) => return Err(Box::new(e)),
        }
        .to_vec();
        Ok(HttpResponse {
            body,
            headers,
            status,
            url,
            version: version.try_into()?,
        })
    }
}

// Converts an [`HttpResponse`] to a reqwest [`Response`]
fn convert_response(response: HttpResponse) -> Result<Response> {
    let mut ret_res = http::Response::builder()
        .status(response.status)
        .url(response.url)
        .version(response.version.into())
        .body(response.body)?;
    for header in response.headers {
        ret_res.headers_mut().insert(
            HeaderName::from_str(header.0.clone().as_str())?,
            HeaderValue::from_str(header.1.clone().as_str())?,
        );
    }
    Ok(Response::from(ret_res))
}

#[cfg(feature = "streaming")]
// Converts a reqwest Response to an http::Response with Full body for streaming cache processing
async fn convert_reqwest_response_to_http_full_body(
    response: Response,
) -> Result<http::Response<http_body_util::Full<bytes::Bytes>>> {
    let status = response.status();
    let version = response.version();
    let headers = response.headers().clone();
    let body_bytes = response.bytes().await.map_err(BoxError::from)?;

    let mut http_response =
        http::Response::builder().status(status).version(version);

    for (name, value) in headers.iter() {
        http_response = http_response.header(name, value);
    }

    http_response
        .body(http_body_util::Full::new(body_bytes))
        .map_err(BoxError::from)
}

#[cfg(feature = "streaming")]
// Converts reqwest Response to http response parts (for 304 handling)
fn convert_reqwest_response_to_http_parts(
    response: Response,
) -> Result<(http::response::Parts, ())> {
    let status = response.status();
    let version = response.version();
    let headers = response.headers().clone();

    let mut http_response =
        http::Response::builder().status(status).version(version);

    for (name, value) in headers.iter() {
        http_response = http_response.header(name, value);
    }

    let response = http_response.body(()).map_err(BoxError::from)?;
    Ok(response.into_parts())
}

#[cfg(feature = "streaming")]
// Converts a streaming response back to reqwest Response by buffering it
async fn convert_streaming_response_to_reqwest_buffered<B>(
    response: http::Response<B>,
) -> Result<Response>
where
    B: http_body::Body + Send + 'static,
{
    use http_body_util::BodyExt;

    let (parts, body) = response.into_parts();

    // Collect the streaming body into bytes, handling errors generically
    let body_bytes = match BodyExt::collect(body).await {
        Ok(collected) => collected.to_bytes().to_vec(),
        Err(_) => {
            return Err(BoxError::from("Failed to collect streaming body"))
        }
    };

    let mut http_response =
        http::Response::builder().status(parts.status).version(parts.version);

    for (name, value) in parts.headers.iter() {
        http_response = http_response.header(name, value);
    }

    let response = http_response.body(body_bytes)?;
    Ok(Response::from(response))
}
fn bad_header(e: reqwest::header::InvalidHeaderValue) -> Error {
    Error::Middleware(anyhow!(e))
}

fn from_box_error(e: BoxError) -> Error {
    Error::Middleware(anyhow!(e))
}

#[async_trait::async_trait]
impl<T: CacheManager> reqwest_middleware::Middleware for Cache<T> {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> std::result::Result<Response, Error> {
        let mut middleware = ReqwestMiddleware { req, next, extensions };
        if self
            .0
            .can_cache_request(&middleware)
            .map_err(|e| Error::Middleware(anyhow!(e)))?
        {
            let res = self.0.run(middleware).await.map_err(from_box_error)?;
            let converted = convert_response(res)
                .map_err(|e| Error::Middleware(anyhow!("{}", e)))?;
            Ok(converted)
        } else {
            self.0
                .run_no_cache(&mut middleware)
                .await
                .map_err(from_box_error)?;
            let mut res = middleware
                .next
                .run(middleware.req, middleware.extensions)
                .await?;

            let miss =
                HeaderValue::from_str(HitOrMiss::MISS.to_string().as_ref())
                    .map_err(bad_header)?;
            res.headers_mut().insert(XCACHE, miss.clone());
            res.headers_mut().insert(XCACHELOOKUP, miss);
            Ok(res)
        }
    }
}

#[cfg(feature = "streaming")]
#[async_trait::async_trait]
impl<T: StreamingCacheManager> reqwest_middleware::Middleware
    for StreamingCache<T>
where
    T::Body: Send + 'static,
    <T::Body as http_body::Body>::Data: Send,
    <T::Body as http_body::Body>::Error:
        Into<http_cache::StreamingError> + Send + Sync + 'static,
{
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> std::result::Result<Response, Error> {
        use http_cache::HttpCacheStreamInterface;

        // Convert reqwest Request to http::Request for analysis
        let copied_req = clone_req(&req)?;
        let http_req = match http::Request::try_from(copied_req) {
            Ok(r) => r,
            Err(e) => return Err(Error::Middleware(anyhow!(e))),
        };
        let (parts, _) = http_req.into_parts();

        // Check for mode override from extensions
        let mode_override = extensions.get::<CacheMode>().cloned();

        // Analyze the request for caching behavior
        let analysis = match self.cache.analyze_request(&parts, mode_override) {
            Ok(a) => a,
            Err(e) => return Err(Error::Middleware(anyhow!(e))),
        };

        // Check if we should bypass cache entirely
        if !analysis.should_cache {
            let response = next.run(req, extensions).await?;
            return Ok(response);
        }

        // Look up cached response
        if let Some((cached_response, policy)) = self
            .cache
            .lookup_cached_response(&analysis.cache_key)
            .await
            .map_err(|e| Error::Middleware(anyhow!(e)))?
        {
            // Check if cached response is still fresh
            use http_cache_semantics::BeforeRequest;
            let before_req = policy.before_request(&parts, SystemTime::now());
            match before_req {
                BeforeRequest::Fresh(_fresh_parts) => {
                    // Convert cached streaming response back to reqwest Response
                    // For now, we need to buffer it since reqwest middleware expects buffered responses
                    return convert_streaming_response_to_reqwest_buffered(
                        cached_response,
                    )
                    .await
                    .map_err(|e| Error::Middleware(anyhow!(e)));
                }
                BeforeRequest::Stale { request: conditional_parts, .. } => {
                    // Create conditional request
                    let mut conditional_req = req;
                    for (name, value) in conditional_parts.headers.iter() {
                        conditional_req
                            .headers_mut()
                            .insert(name.clone(), value.clone());
                    }

                    let conditional_response =
                        next.run(conditional_req, extensions).await?;

                    if conditional_response.status() == 304 {
                        // Convert reqwest response parts for handling not modified
                        let (fresh_parts, _) =
                            convert_reqwest_response_to_http_parts(
                                conditional_response,
                            )
                            .map_err(|e| Error::Middleware(anyhow!("{}", e)))?;
                        let updated_response = self
                            .cache
                            .handle_not_modified(cached_response, &fresh_parts)
                            .await
                            .map_err(|e| Error::Middleware(anyhow!(e)))?;

                        return convert_streaming_response_to_reqwest_buffered(
                            updated_response,
                        )
                        .await
                        .map_err(|e| Error::Middleware(anyhow!(e)));
                    } else {
                        // Fresh response received, process it through the cache
                        let http_response =
                            convert_reqwest_response_to_http_full_body(
                                conditional_response,
                            )
                            .await
                            .map_err(|e| Error::Middleware(anyhow!("{}", e)))?;
                        let cached_response = self
                            .cache
                            .process_response(analysis, http_response)
                            .await
                            .map_err(|e| Error::Middleware(anyhow!(e)))?;

                        return convert_streaming_response_to_reqwest_buffered(
                            cached_response,
                        )
                        .await
                        .map_err(|e| Error::Middleware(anyhow!(e)));
                    }
                }
            }
        }

        // Fetch fresh response from upstream
        let response = next.run(req, extensions).await?;
        let http_response =
            convert_reqwest_response_to_http_full_body(response)
                .await
                .map_err(|e| Error::Middleware(anyhow!("{}", e)))?;

        // Process and potentially cache the response
        let cached_response = self
            .cache
            .process_response(analysis, http_response)
            .await
            .map_err(|e| Error::Middleware(anyhow!(e)))?;

        convert_streaming_response_to_reqwest_buffered(cached_response)
            .await
            .map_err(|e| Error::Middleware(anyhow!(e)))
    }
}

#[cfg(test)]
mod test;
