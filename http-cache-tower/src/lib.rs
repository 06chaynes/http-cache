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
//! use http_cache_tower::{HttpCacheLayer, CACacheManager};
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
//!     let cache_manager = CACacheManager::new("./cache".into(), true);
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
//! ```rust
//! use http_cache_tower::{HttpCacheLayer, CACacheManager};
//! use http_cache::{CacheMode, HttpCache, HttpCacheOptions};
//!
//! # #[tokio::main]
//! # async fn main() {
//! // Create cache manager
//! let cache_manager = CACacheManager::new("./cache".into(), true);
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
//! ```rust
//! use http_cache_tower::HttpCacheStreamingLayer;
//! use http_cache::StreamingManager;
//! use std::path::PathBuf;
//!
//! # #[tokio::main]
//! # async fn main() {
//! // Create streaming cache setup
//! let streaming_manager = StreamingManager::new("./streaming-cache".into());
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
//! use http_cache_tower::{HttpCacheLayer, CACacheManager};
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
//!     let cache_manager = CACacheManager::new("./cache".into(), true);
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
use http::{Request, Response};
use http_body::Body;
use http_body_util::BodyExt;
#[cfg(feature = "manager-cacache")]
pub use http_cache::CACacheManager;
#[cfg(feature = "streaming")]
use http_cache::StreamingError;
#[allow(unused_imports)]
use http_cache::{
    CacheManager, CacheMode, HttpCache, HttpCacheInterface, HttpCacheOptions,
    ResponseCacheModeFn,
};
#[cfg(feature = "streaming")]
use http_cache::{
    HttpCacheStreamInterface, HttpStreamingCache, StreamingCacheManager,
};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tower::{Layer, Service, ServiceExt};

pub mod error;
pub use error::HttpCacheError;
#[cfg(feature = "streaming")]
pub use error::TowerStreamingError;

/// Helper function to collect a body into bytes
async fn collect_body<B>(body: B) -> Result<Vec<u8>, B::Error>
where
    B: Body,
{
    let collected = BodyExt::collect(body).await?;
    Ok(collected.to_bytes().to_vec())
}

/// HTTP cache layer for Tower services.
///
/// This layer implements HTTP caching according to RFC 7234, automatically caching
/// GET and HEAD responses based on their cache-control headers and invalidating
/// cache entries when unsafe methods (PUT, POST, DELETE, PATCH) are used.
///
/// # Example
///
/// ```rust
/// use http_cache_tower::{HttpCacheLayer, CACacheManager};
/// use tower::ServiceBuilder;
/// use tower::service_fn;
/// use http::{Request, Response};
/// use http_body_util::Full;
/// use bytes::Bytes;
/// use std::convert::Infallible;
///
/// # #[tokio::main]
/// # async fn main() {
/// let cache_manager = CACacheManager::new("./cache".into(), true);
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
    /// ```rust
    /// use http_cache_tower::{HttpCacheLayer, CACacheManager};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let cache_manager = CACacheManager::new("./cache".into(), true);
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
    /// ```rust
    /// use http_cache_tower::{HttpCacheLayer, CACacheManager};
    /// use http_cache::HttpCacheOptions;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let cache_manager = CACacheManager::new("./cache".into(), true);
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
    /// ```rust
    /// use http_cache_tower::{HttpCacheLayer, CACacheManager};
    /// use http_cache::{HttpCache, CacheMode, HttpCacheOptions};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let cache_manager = CACacheManager::new("./cache".into(), true);
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
/// ```rust
/// use http_cache_tower::HttpCacheStreamingLayer;
/// use http_cache::StreamingManager;
/// use tower::ServiceBuilder;
/// use tower::service_fn;
/// use http_cache::StreamingBody;
/// use http::{Request, Response};
/// use http_body_util::Full;
/// use bytes::Bytes;
/// use std::convert::Infallible;
///
/// # #[tokio::main]
/// # async fn main() {
/// let streaming_manager = StreamingManager::new("./cache".into());
/// let streaming_layer = HttpCacheStreamingLayer::new(streaming_manager);
///
/// // Use with ServiceBuilder
/// let service = ServiceBuilder::new()
///     .layer(streaming_layer)
///     .service_fn(|_req: Request<Full<Bytes>>| async {
///         Ok::<_, Infallible>(Response::new(StreamingBody::<http_body_util::combinators::BoxBody<Bytes, http_cache_tower::TowerStreamingError>>::buffered(Bytes::from("Hello"))))
///     });
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
    /// ```rust
    /// use http_cache_tower::HttpCacheStreamingLayer;
    /// use http_cache::StreamingManager;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let streaming_manager = StreamingManager::new("./cache".into());
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
    /// ```rust
    /// use http_cache_tower::HttpCacheStreamingLayer;
    /// use http_cache::{StreamingManager, HttpCacheOptions};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let streaming_manager = StreamingManager::new("./cache".into());
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
    /// ```rust
    /// use http_cache_tower::HttpCacheStreamingLayer;
    /// use http_cache::{StreamingManager, HttpStreamingCache, CacheMode, HttpCacheOptions};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let streaming_manager = StreamingManager::new("./cache".into());
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
        self.inner
            .poll_ready(cx)
            .map_err(|e| HttpCacheError::HttpError(e.into()))
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let cache = self.cache.clone();
        let (parts, body) = req.into_parts();
        let inner_service = self.inner.clone();

        Box::pin(async move {
            // Analyze the request for caching behavior
            let analysis = cache
                .analyze_request(&parts, None)
                .map_err(|e| HttpCacheError::CacheError(e.to_string()))?;

            // Bust cache keys if needed (this should happen regardless of whether the request is cacheable)
            for key in &analysis.cache_bust_keys {
                cache
                    .manager
                    .delete(key)
                    .await
                    .map_err(|e| HttpCacheError::CacheError(e.to_string()))?;
            }

            // For non-GET/HEAD requests, invalidate cached GET responses for the same resource
            if !analysis.should_cache
                && (parts.method != "GET" && parts.method != "HEAD")
            {
                // Use the cache's options to generate consistent cache key for GET invalidation
                let get_cache_key = cache
                    .options
                    .create_cache_key_for_invalidation(&parts, "GET");
                // Ignore errors if the cache entry doesn't exist
                let _ = cache.manager.delete(&get_cache_key).await;
            }

            // Check if we should bypass cache entirely
            if !analysis.should_cache {
                let req = Request::from_parts(parts, body);
                let response = inner_service
                    .oneshot(req)
                    .await
                    .map_err(|e| HttpCacheError::HttpError(e.into()))?;
                let (parts, body) = response.into_parts();
                return Ok(Response::from_parts(
                    parts,
                    HttpCacheBody::Original(body),
                ));
            }

            // For Reload mode, always bypass cache and fetch fresh
            if analysis.cache_mode == CacheMode::Reload {
                let req = Request::from_parts(parts, body);
                let response = inner_service
                    .oneshot(req)
                    .await
                    .map_err(|e| HttpCacheError::HttpError(e.into()))?;
                let (res_parts, res_body) = response.into_parts();

                // Collect body for processing
                let body_bytes = collect_body(res_body)
                    .await
                    .map_err(|e| HttpCacheError::HttpError(e.into()))?;

                // Process and potentially cache the response
                let cached_response = cache
                    .process_response(
                        analysis,
                        Response::from_parts(
                            res_parts.clone(),
                            body_bytes.clone(),
                        ),
                    )
                    .await
                    .map_err(|e| HttpCacheError::CacheError(e.to_string()))?;

                let (final_parts, final_body) = cached_response.into_parts();
                return Ok(Response::from_parts(
                    final_parts,
                    HttpCacheBody::Buffered(final_body),
                ));
            }

            // Look up cached response
            if let Some((cached_response, policy)) = cache
                .lookup_cached_response(&analysis.cache_key)
                .await
                .map_err(|e| HttpCacheError::CacheError(e.to_string()))?
            {
                // Check if cached response is still fresh
                use http_cache_semantics::BeforeRequest;
                let before_req =
                    policy.before_request(&parts, std::time::SystemTime::now());
                match before_req {
                    BeforeRequest::Fresh(_fresh_parts) => {
                        // Return cached response using helper
                        let body_clone = cached_response.body.clone();
                        let response = http_cache::HttpCacheOptions::http_response_to_response(
                            &cached_response,
                            HttpCacheBody::Buffered(body_clone),
                        )
                        .map_err(HttpCacheError::HttpError)?;
                        return Ok(response);
                    }
                    BeforeRequest::Stale {
                        request: conditional_parts, ..
                    } => {
                        // Create conditional request using original body
                        let conditional_req =
                            Request::from_parts(conditional_parts, body);
                        let conditional_response = inner_service
                            .oneshot(conditional_req)
                            .await
                            .map_err(|e| HttpCacheError::HttpError(e.into()))?;

                        if conditional_response.status() == 304 {
                            // Use cached response but with updated headers from 304 response
                            let (conditional_parts, _) =
                                conditional_response.into_parts();
                            let updated_response = cache
                                .handle_not_modified(
                                    cached_response,
                                    &conditional_parts,
                                )
                                .await
                                .map_err(|e| {
                                    HttpCacheError::CacheError(e.to_string())
                                })?;

                            let body_clone = updated_response.body.clone();
                            let response = http_cache::HttpCacheOptions::http_response_to_response(
                                &updated_response,
                                HttpCacheBody::Buffered(body_clone),
                            )
                            .map_err(HttpCacheError::HttpError)?;
                            return Ok(response);
                        } else {
                            // Fresh response received, process it normally
                            let (parts, res_body) =
                                conditional_response.into_parts();

                            // Collect body and cache the response
                            let body_bytes =
                                collect_body(res_body).await.map_err(|e| {
                                    HttpCacheError::HttpError(e.into())
                                })?;

                            // Process and cache the response
                            let cached_response = cache
                                .process_response(
                                    analysis,
                                    Response::from_parts(
                                        parts.clone(),
                                        body_bytes.clone(),
                                    ),
                                )
                                .await
                                .map_err(|e| {
                                    HttpCacheError::CacheError(e.to_string())
                                })?;

                            let (final_parts, final_body) =
                                cached_response.into_parts();
                            return Ok(Response::from_parts(
                                final_parts,
                                HttpCacheBody::Buffered(final_body),
                            ));
                        }
                    }
                }
            }

            // Fetch fresh response from upstream
            let req = Request::from_parts(parts, body);
            let response = inner_service
                .oneshot(req)
                .await
                .map_err(|e| HttpCacheError::HttpError(e.into()))?;
            let (res_parts, res_body) = response.into_parts();

            // Collect body for processing
            let body_bytes = collect_body(res_body)
                .await
                .map_err(|e| HttpCacheError::HttpError(e.into()))?;

            // Process and potentially cache the response
            let cached_response = cache
                .process_response(
                    analysis,
                    Response::from_parts(res_parts.clone(), body_bytes.clone()),
                )
                .await
                .map_err(|e| HttpCacheError::CacheError(e.to_string()))?;

            let (final_parts, final_body) = cached_response.into_parts();
            Ok(Response::from_parts(
                final_parts,
                HttpCacheBody::Buffered(final_body),
            ))
        })
    }
}

// Hyper service implementation for HttpCacheService
impl<S, CM> hyper::service::Service<Request<hyper::body::Incoming>>
    for HttpCacheService<S, CM>
where
    S: Service<Request<hyper::body::Incoming>> + Clone + Send + 'static,
    S::Response: Into<Response<http_body_util::Full<Bytes>>>,
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

    fn call(&self, _req: Request<hyper::body::Incoming>) -> Self::Future {
        // Convert to the format expected by the generic Service implementation
        let service_clone = self.clone();
        Box::pin(async move { service_clone.call(_req).await })
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
        self.inner
            .poll_ready(cx)
            .map_err(|e| HttpCacheError::HttpError(e.into()))
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let cache = self.cache.clone();
        let (parts, body) = req.into_parts();
        let inner_service = self.inner.clone();

        Box::pin(async move {
            // Analyze the request for caching behavior
            let analysis = cache
                .analyze_request(&parts, None)
                .map_err(|e| HttpCacheError::CacheError(e.to_string()))?;

            // Bust cache keys if needed (this should happen regardless of whether the request is cacheable)
            for key in &analysis.cache_bust_keys {
                cache
                    .manager
                    .delete(key)
                    .await
                    .map_err(|e| HttpCacheError::CacheError(e.to_string()))?;
            }

            // For non-GET/HEAD requests, invalidate cached GET responses for the same resource
            if !analysis.should_cache
                && (parts.method != "GET" && parts.method != "HEAD")
            {
                // Use the cache's options to generate consistent cache key for GET invalidation
                let get_cache_key = cache
                    .options
                    .create_cache_key_for_invalidation(&parts, "GET");
                // Ignore errors if the cache entry doesn't exist
                let _ = cache.manager.delete(&get_cache_key).await;
            }

            // Check if we should bypass cache entirely
            if !analysis.should_cache {
                let req = Request::from_parts(parts, body);
                let response = inner_service
                    .oneshot(req)
                    .await
                    .map_err(|e| HttpCacheError::HttpError(e.into()))?;
                // For non-cacheable responses, use the manager's convert_body method
                let converted_response =
                    cache.manager.convert_body(response).await.map_err(
                        |e| HttpCacheError::CacheError(e.to_string()),
                    )?;
                return Ok(converted_response);
            }

            // Look up cached response
            if let Some((cached_response, policy)) = cache
                .lookup_cached_response(&analysis.cache_key)
                .await
                .map_err(|e| HttpCacheError::CacheError(e.to_string()))?
            {
                // Check if cached response is still fresh
                use http_cache_semantics::BeforeRequest;
                let before_req =
                    policy.before_request(&parts, std::time::SystemTime::now());
                match before_req {
                    BeforeRequest::Fresh(_fresh_parts) => {
                        // Return cached response - it's already in the right format
                        return Ok(cached_response);
                    }
                    BeforeRequest::Stale {
                        request: conditional_parts, ..
                    } => {
                        // Create conditional request using original body
                        let conditional_req =
                            Request::from_parts(conditional_parts, body);
                        let conditional_response = inner_service
                            .oneshot(conditional_req)
                            .await
                            .map_err(|e| HttpCacheError::HttpError(e.into()))?;

                        if conditional_response.status() == 304 {
                            // Use cached response with updated headers from 304 response
                            let (conditional_parts, _) =
                                conditional_response.into_parts();
                            let updated_response = cache
                                .handle_not_modified(
                                    cached_response,
                                    &conditional_parts,
                                )
                                .await
                                .map_err(|e| {
                                    HttpCacheError::CacheError(e.to_string())
                                })?;
                            return Ok(updated_response);
                        } else {
                            // Fresh response received, process it normally
                            let cached_response = cache
                                .process_response(
                                    analysis,
                                    conditional_response,
                                )
                                .await
                                .map_err(|e| {
                                    HttpCacheError::CacheError(e.to_string())
                                })?;
                            return Ok(cached_response);
                        }
                    }
                }
            }

            // Fetch fresh response from upstream
            let req = Request::from_parts(parts, body);
            let response = inner_service
                .oneshot(req)
                .await
                .map_err(|e| HttpCacheError::HttpError(e.into()))?;

            // Process and potentially cache the response
            let cached_response = cache
                .process_response(analysis, response)
                .await
                .map_err(|e| HttpCacheError::CacheError(e.to_string()))?;

            Ok(cached_response)
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
