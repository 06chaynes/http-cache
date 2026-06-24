//! HTTP caching middleware for Tower services and Axum applications.
//!
//! This crate provides Tower layers that implement HTTP caching according to RFC 7234.
//! It supports both traditional buffered caching and streaming responses for large payloads.
//!
//! ## Basic Usage
//!
//! ### With Tower Services
//!
//! ```rust,no_run
//! use http_cache_tower::{HttpCacheLayer, RedbManager};
//! use http_cache::{CacheMode, HttpCache, HttpCacheOptions};
//! use tower::ServiceBuilder;
//! use tower::service_fn;
//! use tower::ServiceExt;
//! use http::{Request, Response};
//! use http_body_util::Full;
//! use bytes::Bytes;
//! use std::convert::Infallible;
//!
//! async fn handler(_req: Request<Full<Bytes>>) -> Result<Response<Full<Bytes>>, Infallible> {
//!     Ok(Response::new(Full::new(Bytes::from("Hello, World!"))))
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     // Create cache manager with disk storage
//!     let cache_manager = RedbManager::new("./http-cache.redb").unwrap();
//!     
//!     // Create cache layer
//!     let cache_layer = HttpCacheLayer::new(cache_manager);
//!     
//!     // Build service with caching
//!     let service = ServiceBuilder::new()
//!         .layer(cache_layer)
//!         .service_fn(handler);
//!     
//!     // Use the service
//!     let request = Request::builder()
//!         .uri("http://example.com")
//!         .body(Full::new(Bytes::new()))
//!         .unwrap();
//!     let response = service.oneshot(request).await.unwrap();
//! }
//! ```
//!
//! ### With Custom Cache Configuration
//!
//! ```rust,no_run
//! use http_cache_tower::{HttpCacheLayer, RedbManager};
//! use http_cache::{CacheMode, HttpCache, HttpCacheOptions};
//!
//! # #[tokio::main]
//! # async fn main() {
//! // Create cache manager
//! let cache_manager = RedbManager::new("./http-cache.redb").unwrap();
//!
//! // Configure cache behavior
//! let cache = HttpCache {
//!     mode: CacheMode::Default,
//!     manager: cache_manager,
//!     options: HttpCacheOptions::default(),
//! };
//!
//! // Create layer with custom cache
//! let cache_layer = HttpCacheLayer::with_cache(cache);
//! # }
//! ```
//!
//! ### Streaming Support
//!
//! For handling large responses without buffering, use `StreamingManager`:
//!
//! ```rust,ignore
//! use http_cache_tower::HttpCacheStreamingLayer;
//! use http_cache::StreamingManager;
//!
//! # #[tokio::main]
//! # async fn main() {
//! // Create streaming cache setup
//! let streaming_manager = StreamingManager::with_temp_dir(1000).await.unwrap();
//! let streaming_layer = HttpCacheStreamingLayer::new(streaming_manager);
//!
//! // Use with your service
//! // let service = streaming_layer.layer(your_service);
//! # }
//! ```
//!
//! ## Cache Modes
//!
//! Different cache modes provide different behaviors:
//!
//! - `CacheMode::Default`: Follow HTTP caching rules strictly
//! - `CacheMode::NoStore`: Never cache responses
//! - `CacheMode::NoCache`: Always revalidate with the origin server
//! - `CacheMode::ForceCache`: Cache responses even if headers suggest otherwise
//! - `CacheMode::OnlyIfCached`: Only serve from cache, never hit origin server
//! - `CacheMode::IgnoreRules`: Cache everything regardless of headers
//!
//! ## Cache Invalidation
//!
//! The middleware automatically handles cache invalidation for unsafe HTTP methods:
//!
//! ```text
//! These methods will invalidate any cached GET response for the same URI:
//! - PUT /api/users/123    -> invalidates GET /api/users/123
//! - POST /api/users/123   -> invalidates GET /api/users/123  
//! - DELETE /api/users/123 -> invalidates GET /api/users/123
//! - PATCH /api/users/123  -> invalidates GET /api/users/123
//! ```
//!
//! ## Integration with Other Tower Layers
//!
//! The cache layer works with other Tower middleware:
//!
//! ```rust,no_run
//! use tower::ServiceBuilder;
//! use http_cache_tower::{HttpCacheLayer, RedbManager};
//! use tower::service_fn;
//! use tower::ServiceExt;
//! use http::{Request, Response};
//! use http_body_util::Full;
//! use bytes::Bytes;
//! use std::convert::Infallible;
//!
//! async fn handler(_req: Request<Full<Bytes>>) -> Result<Response<Full<Bytes>>, Infallible> {
//!     Ok(Response::new(Full::new(Bytes::from("Hello, World!"))))
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let cache_manager = RedbManager::new("./http-cache.redb").unwrap();
//!     let cache_layer = HttpCacheLayer::new(cache_manager);
//!
//!     let service = ServiceBuilder::new()
//!         // .layer(TraceLayer::new_for_http())  // Logging (requires tower-http)
//!         // .layer(CompressionLayer::new())     // Compression (requires tower-http)
//!         .layer(cache_layer)                    // Caching
//!         .service_fn(handler);
//!     
//!     // Use the service
//!     let request = Request::builder()
//!         .uri("http://example.com")
//!         .body(Full::new(Bytes::new()))
//!         .unwrap();
//!     let response = service.oneshot(request).await.unwrap();
//! }
//! ```

use bytes::Bytes;
use http::{
    header::CACHE_CONTROL, request, HeaderValue, Method, Request, Response,
};
use http_body::Body;
use http_body_util::BodyExt;

#[cfg(feature = "manager-cacache")]
pub use http_cache::CACacheManager;

#[cfg(feature = "manager-redb")]
pub use http_cache::RedbManager;

#[cfg(feature = "rate-limiting")]
pub use http_cache::rate_limiting::{
    CacheAwareRateLimiter, DirectRateLimiter, DomainRateLimiter, Quota,
};
#[cfg(feature = "streaming")]
use http_cache::StreamingError;
use http_cache::{
    url_parse, BoxError, CacheManager, CacheMode, CacheOptions, HitOrMiss,
    HttpCache, HttpCacheOptions, HttpResponse, Middleware, Url, XCACHE,
    XCACHELOOKUP,
};
#[cfg(feature = "streaming")]
use http_cache::{HttpStreamingCache, StreamingCacheManager};
use http_cache_semantics::CachePolicy;
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::SystemTime,
};
use tower::{Layer, Service, ServiceExt};

// Re-export unified error types from http-cache core
pub use http_cache::HttpCacheError;

#[cfg(feature = "streaming")]
/// Type alias for tower streaming errors, using the unified streaming error system
pub type TowerStreamingError = http_cache::ClientStreamingError;

/// Helper functions for error conversions
trait HttpCacheErrorExt<T> {
    fn cache_err(self) -> Result<T, HttpCacheError>;
}

impl<T, E> HttpCacheErrorExt<T> for Result<T, E>
where
    E: ToString,
{
    fn cache_err(self) -> Result<T, HttpCacheError> {
        self.map_err(|e| HttpCacheError::cache(e.to_string()))
    }
}

/// Helper function to add cache status headers to a response
fn add_cache_status_headers<B>(
    mut response: Response<HttpCacheBody<B>>,
    hit_or_miss: &str,
    cache_lookup: &str,
) -> Response<HttpCacheBody<B>> {
    let headers = response.headers_mut();
    if let Ok(hv) = HeaderValue::from_str(hit_or_miss) {
        headers.insert(XCACHE, hv);
    }
    if let Ok(hv) = HeaderValue::from_str(cache_lookup) {
        headers.insert(XCACHELOOKUP, hv);
    }
    response
}

/// Middleware adapter that bridges Tower services to the `http_cache::Middleware`
/// trait, allowing `HttpCache::run` to drive the full cache flow (mode dispatch,
/// conditional revalidation, 5xx handling, warning headers, etc.) instead of
/// reimplementing it inline.
struct TowerMiddleware<S, ReqBody> {
    parts: request::Parts,
    body: Option<ReqBody>,
    service: Option<S>,
}

impl<S, ReqBody, ResBody> Middleware for TowerMiddleware<S, ReqBody>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send + 'static,
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    ResBody: Body + Send + 'static,
    ResBody::Data: Send,
    ResBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn is_method_get_head(&self) -> bool {
        self.parts.method == Method::GET || self.parts.method == Method::HEAD
    }

    fn policy(
        &self,
        response: &HttpResponse,
    ) -> http_cache::Result<CachePolicy> {
        Ok(CachePolicy::new(&self.parts, &response.parts()?))
    }

    fn policy_with_options(
        &self,
        response: &HttpResponse,
        options: CacheOptions,
    ) -> http_cache::Result<CachePolicy> {
        Ok(CachePolicy::new_options(
            &self.parts,
            &response.parts()?,
            SystemTime::now(),
            options,
        ))
    }

    fn update_headers(
        &mut self,
        parts: &request::Parts,
    ) -> http_cache::Result<()> {
        for (name, value) in parts.headers.iter() {
            self.parts.headers.insert(name.clone(), value.clone());
        }
        Ok(())
    }

    fn force_no_cache(&mut self) -> http_cache::Result<()> {
        self.parts
            .headers
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        Ok(())
    }

    fn parts(&self) -> http_cache::Result<request::Parts> {
        Ok(self.parts.clone())
    }

    fn url(&self) -> http_cache::Result<Url> {
        url_parse(self.parts.uri.to_string().as_str())
    }

    fn method(&self) -> http_cache::Result<String> {
        Ok(self.parts.method.as_ref().to_string())
    }

    async fn remote_fetch(&mut self) -> http_cache::Result<HttpResponse> {
        let body = self
            .body
            .take()
            .ok_or_else(|| BoxError::from("request body already consumed"))?;
        let service = self
            .service
            .take()
            .ok_or_else(|| BoxError::from("inner service already consumed"))?;

        let request = Request::from_parts(self.parts.clone(), body);
        let response = service.oneshot(request).await.map_err(|e| {
            let boxed: Box<dyn std::error::Error + Send + Sync> = e.into();
            boxed
        })?;

        let (res_parts, res_body) = response.into_parts();
        let collected = BodyExt::collect(res_body).await.map_err(|e| {
            let boxed: Box<dyn std::error::Error + Send + Sync> = e.into();
            boxed
        })?;
        let body_bytes = collected.to_bytes().to_vec();

        let url = url_parse(self.parts.uri.to_string().as_str())?;
        let headers = (&res_parts.headers).into();
        let status = res_parts.status.as_u16();
        let version = res_parts.version.try_into()?;

        Ok(HttpResponse {
            body: body_bytes,
            headers,
            status,
            url,
            version,
            metadata: None,
        })
    }
}

/// Convert an [`HttpResponse`] from the cache core into a Tower
/// `Response<HttpCacheBody<B>>`.
fn http_response_to_tower_response<B>(
    http_response: HttpResponse,
) -> Result<Response<HttpCacheBody<B>>, HttpCacheError> {
    let mut response = HttpCacheOptions::http_response_to_response(
        &http_response,
        HttpCacheBody::Buffered(http_response.body.clone()),
    )
    .map_err(HttpCacheError::other)?;

    // Preserve metadata in response extensions
    if let Some(metadata) = http_response.metadata {
        response
            .extensions_mut()
            .insert(http_cache::HttpCacheMetadata::from(metadata));
    }

    Ok(response)
}

#[cfg(feature = "streaming")]
fn add_cache_status_headers_streaming<B>(
    mut response: Response<B>,
    hit_or_miss: &str,
    cache_lookup: &str,
) -> Response<B> {
    let headers = response.headers_mut();
    if let Ok(hv) = HeaderValue::from_str(hit_or_miss) {
        headers.insert(XCACHE, hv);
    }
    if let Ok(hv) = HeaderValue::from_str(cache_lookup) {
        headers.insert(XCACHELOOKUP, hv);
    }
    response
}

/// HTTP cache layer for Tower services.
///
/// This layer implements HTTP caching according to RFC 7234, automatically caching
/// GET and HEAD responses based on their cache-control headers and invalidating
/// cache entries when unsafe methods (PUT, POST, DELETE, PATCH) are used.
///
/// # Example
///
/// ```rust,no_run
/// use http_cache_tower::{HttpCacheLayer, RedbManager};
/// use tower::ServiceBuilder;
/// use tower::service_fn;
/// use http::{Request, Response};
/// use http_body_util::Full;
/// use bytes::Bytes;
/// use std::convert::Infallible;
///
/// # #[tokio::main]
/// # async fn main() {
/// let cache_manager = RedbManager::new("./http-cache.redb").unwrap();
/// let cache_layer = HttpCacheLayer::new(cache_manager);
///
/// // Use with ServiceBuilder
/// let service = ServiceBuilder::new()
///     .layer(cache_layer)
///     .service_fn(|_req: Request<Full<Bytes>>| async {
///         Ok::<_, Infallible>(Response::new(Full::new(Bytes::from("Hello"))))
///     });
/// # }
/// ```
#[derive(Clone)]
pub struct HttpCacheLayer<CM>
where
    CM: CacheManager,
{
    cache: Arc<HttpCache<CM>>,
}

impl<CM> HttpCacheLayer<CM>
where
    CM: CacheManager,
{
    /// Create a new HTTP cache layer with default configuration.
    ///
    /// Uses [`CacheMode::Default`] and default [`HttpCacheOptions`].
    ///
    /// # Arguments
    ///
    /// * `cache_manager` - The cache manager to use for storing responses
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use http_cache_tower::{HttpCacheLayer, RedbManager};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let cache_manager = RedbManager::new("./http-cache.redb").unwrap();
    /// let layer = HttpCacheLayer::new(cache_manager);
    /// # }
    /// ```
    pub fn new(cache_manager: CM) -> Self {
        Self {
            cache: Arc::new(HttpCache {
                mode: CacheMode::Default,
                manager: cache_manager,
                options: HttpCacheOptions::default(),
            }),
        }
    }

    /// Create a new HTTP cache layer with custom options.
    ///
    /// Uses [`CacheMode::Default`] but allows customizing the cache behavior
    /// through [`HttpCacheOptions`].
    ///
    /// # Arguments
    ///
    /// * `cache_manager` - The cache manager to use for storing responses
    /// * `options` - Custom cache options
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use http_cache_tower::{HttpCacheLayer, RedbManager};
    /// use http_cache::HttpCacheOptions;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let cache_manager = RedbManager::new("./http-cache.redb").unwrap();
    ///
    /// let options = HttpCacheOptions {
    ///     cache_key: Some(std::sync::Arc::new(|req: &http::request::Parts| {
    ///         format!("custom:{}:{}", req.method, req.uri)
    ///     })),
    ///     ..Default::default()
    /// };
    ///
    /// let layer = HttpCacheLayer::with_options(cache_manager, options);
    /// # }
    /// ```
    pub fn with_options(cache_manager: CM, options: HttpCacheOptions) -> Self {
        Self {
            cache: Arc::new(HttpCache {
                mode: CacheMode::Default,
                manager: cache_manager,
                options,
            }),
        }
    }

    /// Create a new HTTP cache layer with a pre-configured cache.
    ///
    /// This method gives you full control over the cache configuration,
    /// including the cache mode.
    ///
    /// # Arguments
    ///
    /// * `cache` - A fully configured HttpCache instance
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use http_cache_tower::{HttpCacheLayer, RedbManager};
    /// use http_cache::{HttpCache, CacheMode, HttpCacheOptions};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let cache_manager = RedbManager::new("./http-cache.redb").unwrap();
    ///
    /// let cache = HttpCache {
    ///     mode: CacheMode::ForceCache,
    ///     manager: cache_manager,
    ///     options: HttpCacheOptions::default(),
    /// };
    ///
    /// let layer = HttpCacheLayer::with_cache(cache);
    /// # }
    /// ```
    pub fn with_cache(cache: HttpCache<CM>) -> Self {
        Self { cache: Arc::new(cache) }
    }
}

/// HTTP cache layer with streaming support for Tower services.
///
/// This layer provides the same HTTP caching functionality as [`HttpCacheLayer`]
/// but handles streaming responses. It can work with large
/// responses without buffering them entirely in memory.
///
/// # Example
///
/// ```rust,no_run
/// use http_cache_tower::HttpCacheStreamingLayer;
/// use http_cache::StreamingManager;
/// use tower::ServiceBuilder;
/// use tower::service_fn;
/// use http::{Request, Response};
/// use http_body_util::Full;
/// use bytes::Bytes;
/// use std::convert::Infallible;
///
/// async fn handler(_req: Request<Full<Bytes>>) -> Result<Response<Full<Bytes>>, Infallible> {
///     Ok(Response::new(Full::new(Bytes::from("Hello"))))
/// }
///
/// # #[tokio::main]
/// # async fn main() {
/// let streaming_manager = StreamingManager::with_temp_dir(1000).await.unwrap();
/// let streaming_layer = HttpCacheStreamingLayer::new(streaming_manager);
///
/// // Use with ServiceBuilder
/// let service = ServiceBuilder::new()
///     .layer(streaming_layer)
///     .service_fn(handler);
/// # }
/// ```
#[cfg(feature = "streaming")]
#[derive(Clone)]
pub struct HttpCacheStreamingLayer<CM>
where
    CM: StreamingCacheManager,
{
    cache: Arc<HttpStreamingCache<CM>>,
}

#[cfg(feature = "streaming")]
impl<CM> HttpCacheStreamingLayer<CM>
where
    CM: StreamingCacheManager,
{
    /// Create a new HTTP cache streaming layer with default configuration.
    ///
    /// Uses [`CacheMode::Default`] and default [`HttpCacheOptions`].
    ///
    /// # Arguments
    ///
    /// * `cache_manager` - The streaming cache manager to use
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use http_cache_tower::HttpCacheStreamingLayer;
    /// use http_cache::StreamingManager;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let streaming_manager = StreamingManager::with_temp_dir(1000).await.unwrap();
    /// let layer = HttpCacheStreamingLayer::new(streaming_manager);
    /// # }
    /// ```
    pub fn new(cache_manager: CM) -> Self {
        Self {
            cache: Arc::new(HttpStreamingCache {
                mode: CacheMode::Default,
                manager: cache_manager,
                options: HttpCacheOptions::default(),
            }),
        }
    }

    /// Create a new HTTP cache streaming layer with custom options.
    ///
    /// Uses [`CacheMode::Default`] but allows customizing cache behavior.
    ///
    /// # Arguments
    ///
    /// * `cache_manager` - The streaming cache manager to use
    /// * `options` - Custom cache options
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use http_cache_tower::HttpCacheStreamingLayer;
    /// use http_cache::{StreamingManager, HttpCacheOptions};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let streaming_manager = StreamingManager::with_temp_dir(1000).await.unwrap();
    ///
    /// let options = HttpCacheOptions {
    ///     cache_key: Some(std::sync::Arc::new(|req: &http::request::Parts| {
    ///         format!("stream:{}:{}", req.method, req.uri)
    ///     })),
    ///     ..Default::default()
    /// };
    ///
    /// let layer = HttpCacheStreamingLayer::with_options(streaming_manager, options);
    /// # }
    /// ```
    pub fn with_options(cache_manager: CM, options: HttpCacheOptions) -> Self {
        Self {
            cache: Arc::new(HttpStreamingCache {
                mode: CacheMode::Default,
                manager: cache_manager,
                options,
            }),
        }
    }

    /// Create a new HTTP cache streaming layer with a pre-configured cache.
    ///
    /// This method gives you full control over the streaming cache configuration.
    ///
    /// # Arguments
    ///
    /// * `cache` - A fully configured HttpStreamingCache instance
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use http_cache_tower::HttpCacheStreamingLayer;
    /// use http_cache::{StreamingManager, HttpStreamingCache, CacheMode, HttpCacheOptions};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let streaming_manager = StreamingManager::with_temp_dir(1000).await.unwrap();
    ///
    /// let cache = HttpStreamingCache {
    ///     mode: CacheMode::ForceCache,
    ///     manager: streaming_manager,
    ///     options: HttpCacheOptions::default(),
    /// };
    ///
    /// let layer = HttpCacheStreamingLayer::with_cache(cache);
    /// # }
    /// ```
    pub fn with_cache(cache: HttpStreamingCache<CM>) -> Self {
        Self { cache: Arc::new(cache) }
    }
}

impl<S, CM> Layer<S> for HttpCacheLayer<CM>
where
    CM: CacheManager,
{
    type Service = HttpCacheService<S, CM>;

    fn layer(&self, inner: S) -> Self::Service {
        HttpCacheService { inner, cache: self.cache.clone() }
    }
}

#[cfg(feature = "streaming")]
impl<S, CM> Layer<S> for HttpCacheStreamingLayer<CM>
where
    CM: StreamingCacheManager,
{
    type Service = HttpCacheStreamingService<S, CM>;

    fn layer(&self, inner: S) -> Self::Service {
        HttpCacheStreamingService { inner, cache: self.cache.clone() }
    }
}

/// HTTP cache service for Tower/Hyper
pub struct HttpCacheService<S, CM>
where
    CM: CacheManager,
{
    inner: S,
    cache: Arc<HttpCache<CM>>,
}

impl<S, CM> Clone for HttpCacheService<S, CM>
where
    S: Clone,
    CM: CacheManager,
{
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone(), cache: self.cache.clone() }
    }
}

/// HTTP cache streaming service for Tower/Hyper
#[cfg(feature = "streaming")]
pub struct HttpCacheStreamingService<S, CM>
where
    CM: StreamingCacheManager,
{
    inner: S,
    cache: Arc<HttpStreamingCache<CM>>,
}

#[cfg(feature = "streaming")]
impl<S, CM> Clone for HttpCacheStreamingService<S, CM>
where
    S: Clone,
    CM: StreamingCacheManager,
{
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone(), cache: self.cache.clone() }
    }
}

impl<S, CM, ReqBody, ResBody> Service<Request<ReqBody>>
    for HttpCacheService<S, CM>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send + 'static,
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    ResBody: Body + Send + 'static,
    ResBody::Data: Send,
    ResBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    CM: CacheManager,
{
    type Response = Response<HttpCacheBody<ResBody>>;
    type Error = HttpCacheError;
    type Future = Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Self::Response, Self::Error>,
                > + Send,
        >,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(|e| HttpCacheError::http(e.into()))
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let cache = self.cache.clone();
        let (parts, body) = req.into_parts();
        let inner_service = self.inner.clone();

        Box::pin(async move {
            let middleware = TowerMiddleware {
                parts: parts.clone(),
                body: Some(body),
                service: Some(inner_service),
            };

            let can_cache = cache.can_cache_request(&middleware).cache_err()?;

            if can_cache {
                // Delegate the full cache orchestration (mode dispatch,
                // conditional revalidation, 304/5xx handling, warning
                // headers, rate limiting, cache busting) to the core.
                let res = cache.run(middleware).await.cache_err()?;
                http_response_to_tower_response(res)
            } else {
                // Not cacheable -- forward directly, then invalidate on
                // success (RFC 7234 Section 4.4).
                let parts_for_invalidation = middleware.parts().cache_err()?;

                // Reconstruct the request from the middleware's parts.
                let body = middleware.body.ok_or_else(|| {
                    HttpCacheError::cache(
                        "request body already consumed".to_string(),
                    )
                })?;
                let service = middleware.service.ok_or_else(|| {
                    HttpCacheError::cache(
                        "inner service already consumed".to_string(),
                    )
                })?;
                let req = Request::from_parts(parts, body);

                let response = service.oneshot(req).await.map_err(|e| {
                    let boxed: Box<dyn std::error::Error + Send + Sync> =
                        e.into();
                    HttpCacheError::http(boxed)
                })?;

                // Only invalidate for unsafe methods after successful response (RFC 7234 s4.4)
                if !parts_for_invalidation.method.is_safe()
                    && (response.status().is_success()
                        || response.status().is_redirection())
                {
                    cache
                        .run_no_cache_from_parts(&parts_for_invalidation)
                        .await
                        .cache_err()?;
                }

                let mut response = response.map(HttpCacheBody::Original);

                if cache.options.cache_status_headers {
                    response = add_cache_status_headers(
                        response,
                        HitOrMiss::MISS.to_string().as_ref(),
                        HitOrMiss::MISS.to_string().as_ref(),
                    );
                }

                Ok(response)
            }
        })
    }
}

// Hyper service implementation for HttpCacheService
impl<S, CM> hyper::service::Service<Request<hyper::body::Incoming>>
    for HttpCacheService<S, CM>
where
    S: Service<
            Request<hyper::body::Incoming>,
            Response = Response<http_body_util::Full<Bytes>>,
        > + Clone
        + Send
        + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send + 'static,
    CM: CacheManager,
{
    type Response = Response<HttpCacheBody<http_body_util::Full<Bytes>>>;
    type Error = HttpCacheError;
    type Future = Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Self::Response, Self::Error>,
                > + Send,
        >,
    >;

    fn call(&self, req: Request<hyper::body::Incoming>) -> Self::Future {
        // Delegate to the Tower Service impl, which takes &mut self
        let mut service_clone = self.clone();
        Box::pin(
            async move { tower::Service::call(&mut service_clone, req).await },
        )
    }
}

#[cfg(feature = "streaming")]
impl<S, CM, ReqBody, ResBody> Service<Request<ReqBody>>
    for HttpCacheStreamingService<S, CM>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send + 'static,
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send,
    ReqBody::Error: Into<StreamingError>,
    ResBody: Body + Send + 'static,
    ResBody::Data: Send,
    ResBody::Error: Into<StreamingError>,
    CM: StreamingCacheManager,
    <CM::Body as http_body::Body>::Data: Send,
    <CM::Body as http_body::Body>::Error:
        Into<StreamingError> + Send + Sync + 'static,
{
    type Response = Response<CM::Body>;
    type Error = HttpCacheError;
    type Future = Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Self::Response, Self::Error>,
                > + Send,
        >,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(|e| HttpCacheError::http(e.into()))
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let cache = self.cache.clone();
        let (parts, body) = req.into_parts();
        let inner_service = self.inner.clone();

        Box::pin(async move {
            // Check whether this request is cacheable.  Non-cacheable
            // requests (e.g. POST/PUT/DELETE) are forwarded directly and
            // only trigger cache invalidation on success.
            let can_cache =
                cache.can_cache_request(&parts, None).cache_err()?;

            if !can_cache {
                // Forward the request without cache orchestration.
                let req = Request::from_parts(parts.clone(), body);
                let response =
                    inner_service.oneshot(req).await.map_err(|e| {
                        let boxed: Box<dyn std::error::Error + Send + Sync> =
                            e.into();
                        HttpCacheError::http(boxed)
                    })?;

                // Only invalidate for unsafe methods after successful response (RFC 7234 s4.4)
                if !parts.method.is_safe()
                    && (response.status().is_success()
                        || response.status().is_redirection())
                {
                    cache.run_no_cache(&parts).await.cache_err()?;
                }

                let mut converted =
                    cache.manager.convert_body(response).await.cache_err()?;

                if cache.options.cache_status_headers {
                    converted = add_cache_status_headers_streaming(
                        converted, "MISS", "MISS",
                    );
                }

                return Ok(converted);
            }

            // Delegate the full cache orchestration (analyse, lookup,
            // conditional revalidation, 304/200/5xx handling, rate
            // limiting, warning headers, cache busting) to the core
            // library.
            //
            // The closure is `FnOnce` and called at most once.
            // We move `body` and `inner_service` directly into the
            // closure.
            let result = cache
                .run(&parts, None, |fetch_req| {
                    let parts_ref = parts.clone();
                    async move {
                        let request_parts = match fetch_req {
                            http_cache::FetchRequest::Fresh => parts_ref,
                            http_cache::FetchRequest::FreshNoCache => {
                                let mut p = parts_ref;
                                p.headers.insert(
                                    CACHE_CONTROL,
                                    HeaderValue::from_static("no-cache"),
                                );
                                p
                            }
                            http_cache::FetchRequest::Conditional(
                                cond_parts,
                            ) => *cond_parts,
                        };

                        let req = Request::from_parts(request_parts, body);

                        inner_service.oneshot(req).await.map_err(|e| {
                            let boxed: Box<
                                dyn std::error::Error + Send + Sync,
                            > = e.into();
                            boxed
                        })
                    }
                })
                .await
                .cache_err()?;

            Ok(result)
        })
    }
}

/// Body type that wraps cached responses  
pub enum HttpCacheBody<B> {
    /// Buffered body from cache
    Buffered(Vec<u8>),
    /// Original body (fallback)
    Original(B),
}

impl<B> Body for HttpCacheBody<B>
where
    B: Body + Unpin,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B::Data: Into<bytes::Bytes>,
{
    type Data = bytes::Bytes;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        match &mut *self {
            HttpCacheBody::Buffered(bytes) => {
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    let data = std::mem::take(bytes);
                    Poll::Ready(Some(Ok(http_body::Frame::data(
                        bytes::Bytes::from(data),
                    ))))
                }
            }
            HttpCacheBody::Original(body) => {
                Pin::new(body).poll_frame(cx).map(|opt| {
                    opt.map(|res| {
                        res.map(|frame| frame.map_data(Into::into))
                            .map_err(Into::into)
                    })
                })
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            HttpCacheBody::Buffered(bytes) => bytes.is_empty(),
            HttpCacheBody::Original(body) => body.is_end_stream(),
        }
    }

    fn size_hint(&self) -> http_body::SizeHint {
        match self {
            HttpCacheBody::Buffered(bytes) => {
                let len = bytes.len() as u64;
                http_body::SizeHint::with_exact(len)
            }
            HttpCacheBody::Original(body) => body.size_hint(),
        }
    }
}

#[cfg(test)]
mod test;
