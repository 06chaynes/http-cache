//! HTTP caching middleware for Tower services and Axum applications.
//!
//! This crate provides Tower layers that implement HTTP caching according to RFC 7234.
//! It supports both traditional buffered caching and streaming responses for large payloads.
//!
//! ## Basic Usage
//!
//! ### With Axum
//!
//! ```rust,no_run
//! use axum::{Router, routing::get, response::Html};
//! use http_cache_tower::{HttpCacheLayer, CACacheManager};
//! use http_cache::{CacheMode, HttpCache, HttpCacheOptions};
//! use tower::ServiceBuilder;
//!
//! async fn handler() -> Html<&'static str> {
//!     Html("<h1>Hello, World!</h1>")
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
//!     // Build your app with caching
//!     let app = Router::new()
//!         .route("/", get(handler))
//!         .layer(
//!             ServiceBuilder::new()
//!                 .layer(cache_layer)
//!         );
//!     
//!     // Run the server
//!     let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
//!     axum::serve(listener, app).await.unwrap();
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
//! For handling large responses without buffering, use `FileCacheManager`:
//!
//! ```rust
//! use http_cache_tower::HttpCacheStreamingLayer;
//! use http_cache::FileCacheManager;
//! use std::path::PathBuf;
//!
//! # #[tokio::main]
//! # async fn main() {
//! // Create streaming cache setup
//! let cache_manager = FileCacheManager::new(PathBuf::from("./streaming-cache"));
//! let streaming_layer = HttpCacheStreamingLayer::new(cache_manager);
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
//! use axum::Router;
//! use tower::ServiceBuilder;
//! use tower_http::{compression::CompressionLayer, trace::TraceLayer};
//! use http_cache_tower::{HttpCacheLayer, CACacheManager};
//!
//! # async fn handler() {}
//! # #[tokio::main]
//! # async fn main() {
//! let cache_manager = CACacheManager::new("./cache".into(), true);
//! let cache_layer = HttpCacheLayer::new(cache_manager);
//!
//! let app = Router::new()
//!     .route("/", axum::routing::get(handler))
//!     .layer(
//!         ServiceBuilder::new()
//!             .layer(TraceLayer::new_for_http())  // Logging
//!             .layer(CompressionLayer::new())     // Compression
//!             .layer(cache_layer)                 // Caching
//!     );
//! # }
//! ```

use bytes::Bytes;
use http::{Request, Response};
use http_body::Body;
use http_body_util::BodyExt;
#[cfg(feature = "manager-cacache")]
#[allow(unused_imports)]
use http_cache::CACacheManager;
#[cfg(feature = "streaming")]
use http_cache::StreamingError;
#[allow(unused_imports)]
use http_cache::{
    CacheManager, CacheMode, HttpCache, HttpCacheInterface, HttpCacheOptions,
};
#[cfg(feature = "streaming")]
use http_cache::{
    HttpCacheStreamInterface, HttpStreamingCache, StreamingBody,
    StreamingCacheManager,
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

#[cfg(feature = "streaming")]
/// A streaming cache wrapper that adapts regular cache managers to work with streaming responses.
///
/// This wrapper allows regular [`CacheManager`] implementations to work with Tower services
/// that use streaming bodies, by handling the conversion between buffered cached content
/// and streaming bodies.
///
/// # Example
///
/// ```rust
/// use http_cache_tower::StreamingCacheWrapper;
/// use http_cache::CACacheManager;
///
/// # #[tokio::main]
/// # async fn main() {
/// let cache_manager = CACacheManager::new("./cache".into(), true);
/// let streaming_wrapper = StreamingCacheWrapper::new(cache_manager);
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct StreamingCacheWrapper<CM: CacheManager> {
    inner: CM,
}

#[cfg(feature = "streaming")]
impl<CM: CacheManager> StreamingCacheWrapper<CM> {
    /// Create a new streaming cache wrapper around a cache manager.
    ///
    /// # Arguments
    ///
    /// * `cache_manager` - The underlying cache manager to wrap
    ///
    /// # Example
    ///
    /// ```rust
    /// use http_cache_tower::StreamingCacheWrapper;
    /// use http_cache::CACacheManager;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let cache_manager = CACacheManager::new("./cache".into(), true);
    /// let wrapper = StreamingCacheWrapper::new(cache_manager);
    /// # }
    /// ```
    pub fn new(cache_manager: CM) -> Self {
        Self { inner: cache_manager }
    }
}

#[cfg(feature = "streaming")]
#[async_trait::async_trait]
impl<CM: CacheManager> StreamingCacheManager for StreamingCacheWrapper<CM> {
    type Body = StreamingBody<
        http_body_util::combinators::BoxBody<Bytes, TowerStreamingError>,
    >;

    async fn get(
        &self,
        cache_key: &str,
    ) -> http_cache::Result<
        Option<(Response<Self::Body>, http_cache_semantics::CachePolicy)>,
    > {
        if let Some((http_response, policy)) = self.inner.get(cache_key).await?
        {
            // Convert HttpResponse to Response<StreamingBody>
            let mut response_builder = Response::builder()
                .status(http_response.status)
                .version(http_response.version.into());

            for (name, value) in &http_response.headers {
                if let (Ok(header_name), Ok(header_value)) = (
                    name.parse::<http::HeaderName>(),
                    value.parse::<http::HeaderValue>(),
                ) {
                    response_builder =
                        response_builder.header(header_name, header_value);
                }
            }

            let body = StreamingBody::buffered(Bytes::from(http_response.body));
            let response =
                response_builder.body(body).map_err(StreamingError::new)?;

            Ok(Some((response, policy)))
        } else {
            Ok(None)
        }
    }

    async fn put<B>(
        &self,
        cache_key: String,
        response: Response<B>,
        policy: http_cache_semantics::CachePolicy,
        request_url: url::Url,
    ) -> http_cache::Result<Response<Self::Body>>
    where
        B: Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
    {
        let (parts, body) = response.into_parts();

        // Collect the body into memory for caching
        let body_bytes = collect_body(body)
            .await
            .map_err(|e| StreamingError::new(e.into()))?;

        // Convert to HttpResponse format for the underlying cache manager
        // Use the provided request_url directly - this solves the URL reconstruction problem
        let http_response = http_cache::HttpResponse {
            body: body_bytes.clone(),
            headers: parts
                .headers
                .iter()
                .map(|(k, v)| {
                    (k.to_string(), v.to_str().unwrap_or("").to_string())
                })
                .collect(),
            status: parts.status.as_u16(),
            url: request_url,
            version: parts.version.try_into()?,
        };

        // Cache the response
        let cached_response =
            self.inner.put(cache_key, http_response, policy).await?;

        // Convert back to streaming response
        let mut response_builder = Response::builder()
            .status(cached_response.status)
            .version(cached_response.version.into());

        for (name, value) in &cached_response.headers {
            if let (Ok(header_name), Ok(header_value)) = (
                name.parse::<http::HeaderName>(),
                value.parse::<http::HeaderValue>(),
            ) {
                response_builder =
                    response_builder.header(header_name, header_value);
            }
        }

        let body = StreamingBody::buffered(Bytes::from(cached_response.body));
        let response =
            response_builder.body(body).map_err(StreamingError::new)?;

        Ok(response)
    }

    async fn convert_body<B>(
        &self,
        response: Response<B>,
    ) -> http_cache::Result<Response<Self::Body>>
    where
        B: Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
    {
        let (parts, body) = response.into_parts();

        // For non-cacheable responses, we need to collect the body and wrap it
        // This is a limitation but necessary for the generic interface
        let body_bytes = collect_body(body)
            .await
            .map_err(|e| StreamingError::new(e.into()))?;

        let streaming_body = StreamingBody::buffered(Bytes::from(body_bytes));
        let response = Response::from_parts(parts, streaming_body);

        Ok(response)
    }

    async fn delete(&self, cache_key: &str) -> http_cache::Result<()> {
        self.inner.delete(cache_key).await
    }
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
///
/// # #[tokio::main]
/// # async fn main() {
/// let cache_manager = CACacheManager::new("./cache".into(), true);
/// let cache_layer = HttpCacheLayer::new(cache_manager);
///
/// // Use with ServiceBuilder
/// let service = ServiceBuilder::new()
///     .layer(cache_layer)
///     .service(your_service);
/// # }
/// #
/// # fn your_service() -> impl tower::Service<http::Request<http_body_util::Full<bytes::Bytes>>, Response = http::Response<http_body_util::Full<bytes::Bytes>>, Error = Box<dyn std::error::Error + Send + Sync>> + Clone {
/// #     tower::service_fn(|_req| async { Ok(http::Response::new(http_body_util::Full::new(bytes::Bytes::new()))) })
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
    ///     cache_key: Some(Box::new(|req| {
    ///         format!("custom:{}:{}", req.method(), req.uri())
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
/// use http_cache_tower::{HttpCacheStreamingLayer, StreamingCacheWrapper};
/// use tower::ServiceBuilder;
///
/// # #[tokio::main]
/// # async fn main() {
/// let cache_manager = CACacheManager::new("./cache".into(), true);
/// let streaming_wrapper = StreamingCacheWrapper::new(cache_manager);
/// let streaming_layer = HttpCacheStreamingLayer::new(streaming_wrapper);
///
/// // Use with ServiceBuilder
/// let service = ServiceBuilder::new()
///     .layer(streaming_layer)
///     .service(your_streaming_service);
/// # }
/// #
/// # fn your_streaming_service() -> impl tower::Service<http::Request<http_body_util::Full<bytes::Bytes>>, Response = http::Response<http_cache::StreamingBody<http_body_util::combinators::BoxBody<bytes::Bytes, http_cache_tower::TowerStreamingError>>>, Error = Box<dyn std::error::Error + Send + Sync>> + Clone {
/// #     tower::service_fn(|_req| async {
/// #         let body = http_cache::StreamingBody::buffered(bytes::Bytes::new());
/// #         Ok(http::Response::new(body))
/// #     })
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
    /// use http_cache_tower::{HttpCacheStreamingLayer, StreamingCacheWrapper};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let cache_manager = CACacheManager::new("./cache".into(), true);
    /// let streaming_wrapper = StreamingCacheWrapper::new(cache_manager);
    /// let layer = HttpCacheStreamingLayer::new(streaming_wrapper);
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
    /// use http_cache_tower::{HttpCacheStreamingLayer, StreamingCacheWrapper};
    /// use http_cache::HttpCacheOptions;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let cache_manager = CACacheManager::new("./cache".into(), true);
    /// let streaming_wrapper = StreamingCacheWrapper::new(cache_manager);
    ///
    /// let options = HttpCacheOptions {
    ///     cache_key: Some(Box::new(|req| {
    ///         format!("stream:{}:{}", req.method(), req.uri())
    ///     })),
    ///     ..Default::default()
    /// };
    ///
    /// let layer = HttpCacheStreamingLayer::with_options(streaming_wrapper, options);
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
    /// use http_cache_tower::{HttpCacheStreamingLayer, StreamingCacheWrapper};
    /// use http_cache::{HttpStreamingCache, CacheMode, HttpCacheOptions};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let cache_manager = CACacheManager::new("./cache".into(), true);
    /// let streaming_wrapper = StreamingCacheWrapper::new(cache_manager);
    ///
    /// let cache = HttpStreamingCache {
    ///     mode: CacheMode::ForceCache,
    ///     manager: streaming_wrapper,
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
                // Generate the cache key for a GET request to the same resource
                // Default cache key format is "method:uri"
                let get_cache_key = format!("GET:{}", parts.uri);
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
                        // Return cached response
                        let mut response_builder = Response::builder()
                            .status(cached_response.status)
                            .version(cached_response.version.into());

                        for (name, value) in &cached_response.headers {
                            if let (Ok(header_name), Ok(header_value)) = (
                                name.parse::<http::HeaderName>(),
                                value.parse::<http::HeaderValue>(),
                            ) {
                                response_builder = response_builder
                                    .header(header_name, header_value);
                            }
                        }

                        let response = response_builder
                            .body(HttpCacheBody::Buffered(cached_response.body))
                            .map_err(|e| HttpCacheError::HttpError(e.into()))?;
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

                            let mut response_builder = Response::builder()
                                .status(updated_response.status)
                                .version(updated_response.version.into());

                            for (name, value) in &updated_response.headers {
                                if let (Ok(header_name), Ok(header_value)) = (
                                    name.parse::<http::HeaderName>(),
                                    value.parse::<http::HeaderValue>(),
                                ) {
                                    response_builder = response_builder
                                        .header(header_name, header_value);
                                }
                            }

                            let response = response_builder
                                .body(HttpCacheBody::Buffered(
                                    updated_response.body,
                                ))
                                .map_err(|e| {
                                    HttpCacheError::HttpError(e.into())
                                })?;
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
                // Generate the cache key for a GET request to the same resource
                // Default cache key format is "method:uri"
                let get_cache_key = format!("GET:{}", parts.uri);
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
