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
//! use reqwest_middleware::ClientBuilder;
//! use http_cache_reqwest::{Cache, CacheMode, RedbManager, HttpCache, HttpCacheOptions};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: RedbManager::new("./http-cache.redb")?,
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
//! The `StreamingCache` provides streaming support for large responses without buffering
//! them entirely in memory. This is particularly useful for downloading large files or
//! processing streaming APIs while still benefiting from HTTP caching.
//!
//! **Note**: Requires the `streaming` feature and a compatible cache manager that implements
//! [`StreamingCacheManager`]. Currently only the `StreamingCacheManager` supports streaming -
//! `CACacheManager` and `MokaManager` do not support streaming and will buffer responses
//! in memory. The streaming implementation achieves significant memory savings
//! (typically 35-40% reduction) compared to traditional buffered approaches.
//!
//! ```no_run
//! # #[cfg(feature = "streaming")]
//! use reqwest::Client;
//! # #[cfg(feature = "streaming")]
//! use reqwest_middleware::ClientBuilder;
//! # #[cfg(feature = "streaming")]
//! use http_cache_reqwest::{StreamingCache, CacheMode};
//! # #[cfg(feature = "streaming")]
//! use http_cache::StreamingManager;
//!
//! # #[cfg(feature = "streaming")]
//! #[tokio::main]
//! async fn main() -> reqwest_middleware::Result<()> {
//!     let streaming_manager = StreamingManager::with_temp_dir(1000).await.unwrap();
//!     let client = ClientBuilder::new(Client::new())
//!         .with(StreamingCache::new(
//!             streaming_manager,
//!             CacheMode::Default,
//!         ))
//!         .build();
//!         
//!     // Stream large responses efficiently - cached responses are also streamed
//!     let response = client
//!         .get("https://httpbin.org/stream/1000")
//!         .send()
//!         .await?;
//!     println!("Status: {}", response.status());
//!     
//!     // Process the streaming body chunk by chunk
//!     use futures_util::StreamExt;
//!     let mut stream = response.bytes_stream();
//!     while let Some(chunk) = stream.next().await {
//!         let chunk = chunk?;
//!         println!("Received chunk of {} bytes", chunk.len());
//!         // Process chunk without loading entire response into memory
//!     }
//!     
//!     Ok(())
//! }
//! # #[cfg(not(feature = "streaming"))]
//! # fn main() {}
//! ```
//!
//! ### Streaming Cache with Custom Options
//!
//! ```no_run
//! # #[cfg(feature = "streaming")]
//! use reqwest::Client;
//! # #[cfg(feature = "streaming")]
//! use reqwest_middleware::ClientBuilder;
//! # #[cfg(feature = "streaming")]
//! use http_cache_reqwest::{StreamingCache, CacheMode, HttpCacheOptions};
//! # #[cfg(feature = "streaming")]
//! use http_cache::StreamingManager;
//!
//! # #[cfg(feature = "streaming")]
//! #[tokio::main]
//! async fn main() -> reqwest_middleware::Result<()> {
//!     let options = HttpCacheOptions {
//!         cache_bust: Some(std::sync::Arc::new(|req: &http::request::Parts, _cache_key: &Option<std::sync::Arc<dyn Fn(&http::request::Parts) -> String + Send + Sync>>, _uri: &str| {
//!             // Custom cache busting logic for streaming requests
//!             if req.uri.path().contains("/stream/") {
//!                 vec![format!("stream:{}", req.uri)]
//!             } else {
//!                 vec![]
//!             }
//!         })),
//!         ..Default::default()
//!     };
//!
//!     let streaming_manager = StreamingManager::with_temp_dir(1000).await.unwrap();
//!     let client = ClientBuilder::new(Client::new())
//!         .with(StreamingCache::with_options(
//!             streaming_manager,
//!             CacheMode::Default,
//!             options,
//!         ))
//!         .build();
//!         
//!     Ok(())
//! }
//! # #[cfg(not(feature = "streaming"))]
//! # fn main() {}
//! ```
//!
//! ## Cache Modes
//!
//! Control caching behavior with different modes:
//!
//! ```no_run
//! use reqwest::Client;
//! use reqwest_middleware::ClientBuilder;
//! use http_cache_reqwest::{Cache, CacheMode, RedbManager, HttpCache, HttpCacheOptions};
//!
//! #[tokio::main]
//! async fn main() -> reqwest_middleware::Result<()> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::ForceCache, // Cache everything, ignore headers
//!             manager: RedbManager::new("./http-cache.redb").unwrap(),
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
//! use http_cache_reqwest::{Cache, CacheMode, RedbManager, HttpCache, HttpCacheOptions};
//!
//! #[tokio::main]
//! async fn main() -> reqwest_middleware::Result<()> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: RedbManager::new("./http-cache.redb").unwrap(),
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
//! use http_cache_reqwest::{Cache, CacheMode, RedbManager, HttpCache, HttpCacheOptions};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> reqwest_middleware::Result<()> {
//!     let options = HttpCacheOptions {
//!         cache_key: Some(Arc::new(|req: &http::request::Parts| {
//!             // Include query parameters in cache key
//!             format!("{}:{}", req.method, req.uri)
//!         })),
//!         ..Default::default()
//!     };
//!     
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: RedbManager::new("./http-cache.redb").unwrap(),
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
//! use http_cache_reqwest::MokaCache;
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
// Re-export unified error types from http-cache core
pub use http_cache::{BadRequest, HttpCacheError};

#[cfg(feature = "streaming")]
/// Type alias for reqwest streaming errors, using the unified streaming error system
pub type ReqwestStreamingError = http_cache::ClientStreamingError;

#[cfg(feature = "streaming")]
use http_cache::StreamingCacheManager;

use std::{str::FromStr, time::SystemTime};

pub use http::request::Parts;
use http::{
    header::{HeaderName, CACHE_CONTROL},
    Extensions, HeaderValue, Method,
};
use http_cache::{
    url_parse, BoxError, HitOrMiss, Middleware, Result, Url, XCACHE,
    XCACHELOOKUP,
};
use http_cache_semantics::CachePolicy;
use reqwest::{Request, Response, ResponseBuilderExt};
use reqwest_middleware::{Error, Next};

/// Helper function to convert our error types to reqwest middleware errors
fn to_middleware_error<E: std::error::Error + Send + Sync + 'static>(
    error: E,
) -> Error {
    // Convert to anyhow::Error which is what reqwest-middleware expects
    Error::Middleware(anyhow::Error::new(error))
}

pub use http_cache::{
    CacheManager, CacheMode, CacheOptions, HttpCache, HttpCacheMetadata,
    HttpCacheOptions, HttpResponse, MetadataProvider, ResponseCacheModeFn,
};

#[cfg(feature = "streaming")]
// Re-export streaming types for future use
pub use http_cache::{
    HttpCacheStreamInterface, HttpStreamingCache, StreamingBody,
    StreamingManager,
};

#[cfg(feature = "manager-cacache")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-cacache")))]
pub use http_cache::CACacheManager;

#[cfg(feature = "manager-redb")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-redb")))]
pub use http_cache::RedbManager;

#[cfg(feature = "manager-moka")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-moka")))]
pub use http_cache::{MokaCache, MokaCacheBuilder, MokaManager};

#[cfg(feature = "rate-limiting")]
#[cfg_attr(docsrs, doc(cfg(feature = "rate-limiting")))]
pub use http_cache::rate_limiting::{
    CacheAwareRateLimiter, DirectRateLimiter, DomainRateLimiter, Quota,
};

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
        None => Err(to_middleware_error(BadRequest)),
    }
}

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
        // Extract request parts without cloning the body
        let mut builder = http::Request::builder()
            .method(self.req.method().as_str())
            .uri(self.req.url().as_str())
            .version(self.req.version());

        // Add headers
        for (name, value) in self.req.headers() {
            builder = builder.header(name, value);
        }

        // Add extensions
        if let Some(no_error) = builder.extensions_mut() {
            *no_error = self.extensions.clone();
        }

        // Build with empty body just to get the Parts
        let http_req = builder.body(()).map_err(Box::new)?;
        Ok(http_req.into_parts().0)
    }
    fn url(&self) -> Result<Url> {
        // Re-parse the URL through our helper for url/ada-url compatibility
        url_parse(self.req.url().as_str())
    }
    fn method(&self) -> Result<String> {
        Ok(self.req.method().as_ref().to_string())
    }
    async fn remote_fetch(&mut self) -> Result<HttpResponse> {
        let copied_req = clone_req(&self.req)?;
        let res = self
            .next
            .clone()
            .run(copied_req, self.extensions)
            .await
            .map_err(BoxError::from)?;
        let headers = res.headers().into();
        // Re-parse the URL through our helper for url/ada-url compatibility
        let url = url_parse(res.url().as_str())?;
        let status = res.status().into();
        let version = res.version();
        let body: Vec<u8> = res.bytes().await.map_err(BoxError::from)?.to_vec();
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

// Converts an [`HttpResponse`] to a reqwest [`Response`]
fn convert_response(response: HttpResponse) -> Result<Response> {
    let metadata = response.metadata.clone();
    // reqwest always uses url::Url internally, so we need to re-parse when using ada-url
    let reqwest_url =
        ::url::Url::parse(response.url.as_str()).map_err(BoxError::from)?;
    let mut ret_res = http::Response::builder()
        .status(response.status)
        .url(reqwest_url)
        .version(response.version.into())
        .body(response.body)?;
    for header in response.headers {
        ret_res.headers_mut().append(
            HeaderName::from_str(&header.0)?,
            HeaderValue::from_str(&header.1)?,
        );
    }
    // Insert metadata into response extensions if present
    if let Some(metadata) = metadata {
        ret_res.extensions_mut().insert(HttpCacheMetadata::from(metadata));
    }
    Ok(Response::from(ret_res))
}

#[cfg(feature = "streaming")]
/// Final URL of the upstream response, carried through core's orchestrator
/// in response extensions so the reqwest Response rebuilt on the way out
/// reports the real URL instead of reqwest's no.url.provided.local
/// placeholder.
#[derive(Clone)]
struct FinalUrl(::url::Url);

#[cfg(feature = "streaming")]
// Converts a reqwest Response into a genuinely streaming http::Response.
// No body bytes are read here: reqwest::Body implements http_body::Body
// (Data = Bytes, Error = reqwest::Error), so the network stream flows
// through core's orchestrator and into the cache manager frame by frame.
fn convert_reqwest_response_to_streaming(
    response: Response,
) -> http::Response<
    http_body_util::combinators::UnsyncBoxBody<
        bytes::Bytes,
        http_cache::StreamingError,
    >,
> {
    use http_body_util::BodyExt;
    let url = response.url().clone();
    let http_response: http::Response<reqwest::Body> = response.into();
    let (mut parts, body) = http_response.into_parts();
    parts.extensions.insert(FinalUrl(url));
    let body = body.map_err(http_cache::StreamingError::client).boxed_unsync();
    http::Response::from_parts(parts, body)
}

#[cfg(feature = "streaming")]
// Converts a streaming response to reqwest Response using the StreamingCacheManager's method
async fn convert_streaming_body_to_reqwest<T>(
    response: http::Response<T::Body>,
) -> Result<Response>
where
    T: StreamingCacheManager,
    <T::Body as http_body::Body>::Data: Send,
    <T::Body as http_body::Body>::Error: Send + Sync + 'static,
{
    let (mut parts, body) = response.into_parts();
    let final_url = parts.extensions.remove::<FinalUrl>();

    // Use the cache manager's body_to_bytes_stream method for streaming
    let bytes_stream = T::body_to_bytes_stream(body);
    let reqwest_body = reqwest::Body::wrap_stream(bytes_stream);

    let mut builder =
        http::Response::builder().status(parts.status).version(parts.version);
    for (name, value) in parts.headers.iter() {
        builder = builder.header(name, value);
    }
    // Transfer orchestrator extensions (HttpCacheMetadata etc.) into the
    // builder BEFORE applying the URL, so the ResponseUrl the builder
    // inserts is not clobbered.
    if let Some(ext) = builder.extensions_mut() {
        *ext = parts.extensions;
    }
    if let Some(FinalUrl(url)) = final_url {
        builder = builder.url(url);
    }
    let response = builder.body(reqwest_body)?;
    Ok(Response::from(response))
}

fn bad_header(e: reqwest::header::InvalidHeaderValue) -> Error {
    to_middleware_error(HttpCacheError::Cache(e.to_string()))
}

fn from_box_error(e: BoxError) -> Error {
    to_middleware_error(HttpCacheError::Cache(e.to_string()))
}

#[async_trait::async_trait]
impl<T: CacheManager> reqwest_middleware::Middleware for Cache<T> {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> std::result::Result<Response, Error> {
        let middleware = ReqwestMiddleware { req, next, extensions };
        let can_cache =
            self.0.can_cache_request(&middleware).map_err(from_box_error)?;

        if can_cache {
            let res = self.0.run(middleware).await.map_err(from_box_error)?;
            let converted = convert_response(res).map_err(|e| {
                to_middleware_error(HttpCacheError::Cache(e.to_string()))
            })?;
            Ok(converted)
        } else {
            let parts = middleware.parts().map_err(from_box_error)?;
            let mut res = middleware
                .next
                .run(middleware.req, middleware.extensions)
                .await?;

            // Only invalidate for unsafe methods after successful response (RFC 7234 s4.4)
            if !parts.method.is_safe()
                && (res.status().is_success() || res.status().is_redirection())
            {
                self.0
                    .run_no_cache_from_parts(&parts)
                    .await
                    .map_err(from_box_error)?;
            }

            if self.0.options.cache_status_headers {
                let miss =
                    HeaderValue::from_str(HitOrMiss::MISS.to_string().as_ref())
                        .map_err(bad_header)?;
                res.headers_mut().insert(XCACHE, miss.clone());
                res.headers_mut().insert(XCACHELOOKUP, miss);
            }
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
        use http_cache::FetchRequest;

        // Convert reqwest Request to http::Request for analysis.
        // If the request can't be cloned (e.g., streaming body),
        // bypass the cache gracefully.
        let copied_req = match clone_req(&req) {
            Ok(r) => r,
            Err(_) => return next.run(req, extensions).await,
        };
        let http_req = http::Request::try_from(copied_req).map_err(|e| {
            to_middleware_error(HttpCacheError::Cache(e.to_string()))
        })?;
        let (parts, _) = http_req.into_parts();
        let mode_override = extensions.get::<CacheMode>().cloned();

        let can_cache = self
            .cache
            .can_cache_request(&parts, mode_override)
            .map_err(from_box_error)?;

        if can_cache {
            let mut result = self
                .cache
                .run(&parts, mode_override, |fetch_req| {
                    let mut req = req;
                    let next = next.clone();

                    match fetch_req {
                        FetchRequest::Fresh => {}
                        FetchRequest::FreshNoCache => {
                            req.headers_mut().insert(
                                CACHE_CONTROL,
                                HeaderValue::from_static("no-cache"),
                            );
                        }
                        FetchRequest::Conditional(cond_parts) => {
                            for (name, value) in cond_parts.headers.iter() {
                                req.headers_mut()
                                    .insert(name.clone(), value.clone());
                            }
                        }
                    }

                    async move {
                        let resp = next.run(req, extensions).await.map_err(
                            |e| -> BoxError { e.to_string().into() },
                        )?;
                        Ok(convert_reqwest_response_to_streaming(resp))
                    }
                })
                .await
                .map_err(from_box_error)?;

            if result.extensions().get::<FinalUrl>().is_none() {
                if let Ok(u) = ::url::Url::parse(&parts.uri.to_string()) {
                    result.extensions_mut().insert(FinalUrl(u));
                }
            }

            convert_streaming_body_to_reqwest::<T>(result).await.map_err(|e| {
                to_middleware_error(HttpCacheError::Cache(e.to_string()))
            })
        } else {
            let mut res = next.run(req, extensions).await?;

            // Only invalidate for unsafe methods after successful response (RFC 7234 s4.4)
            if !parts.method.is_safe()
                && (res.status().is_success() || res.status().is_redirection())
            {
                self.cache
                    .run_no_cache(&parts)
                    .await
                    .map_err(from_box_error)?;
            }

            if self.cache.options.cache_status_headers {
                let miss =
                    HeaderValue::from_str(HitOrMiss::MISS.to_string().as_ref())
                        .map_err(bad_header)?;
                res.headers_mut().insert(XCACHE, miss.clone());
                res.headers_mut().insert(XCACHELOOKUP, miss);
            }
            Ok(res)
        }
    }
}

#[cfg(test)]
mod test;
