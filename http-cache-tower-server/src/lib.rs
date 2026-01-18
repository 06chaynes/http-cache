//! Server-side HTTP response caching middleware for Tower.
//!
//! This crate provides Tower middleware for caching HTTP responses on the server side.
//! Unlike client-side caching, this middleware caches your own application's responses
//! to reduce load and improve performance.
//!
//! # Key Features
//!
//! - Response-first architecture: Caches based on response headers, not requests
//! - Preserves request context: Maintains all request extensions (path params, state, etc.)
//! - Handler-centric: Calls the handler first, then decides whether to cache
//! - RFC 7234 compliant: Respects Cache-Control, Vary, and other standard headers
//! - Reuses existing infrastructure: Leverages `CacheManager` trait from `http-cache`
//!
//! # Example
//!
//! ```rust
//! use http::{Request, Response};
//! use http_body_util::Full;
//! use bytes::Bytes;
//! use http_cache_tower_server::ServerCacheLayer;
//! use tower::{Service, Layer};
//! # use http_cache::{CacheManager, HttpResponse, HttpVersion};
//! # use http_cache_semantics::CachePolicy;
//! # use std::collections::HashMap;
//! # use std::sync::{Arc, Mutex};
//! #
//! # #[derive(Clone)]
//! # struct MemoryCacheManager {
//! #     store: Arc<Mutex<HashMap<String, (HttpResponse, CachePolicy)>>>,
//! # }
//! #
//! # impl MemoryCacheManager {
//! #     fn new() -> Self {
//! #         Self { store: Arc::new(Mutex::new(HashMap::new())) }
//! #     }
//! # }
//! #
//! # #[async_trait::async_trait]
//! # impl CacheManager for MemoryCacheManager {
//! #     async fn get(&self, cache_key: &str) -> http_cache::Result<Option<(HttpResponse, CachePolicy)>> {
//! #         Ok(self.store.lock().unwrap().get(cache_key).cloned())
//! #     }
//! #     async fn put(&self, cache_key: String, res: HttpResponse, policy: CachePolicy) -> http_cache::Result<HttpResponse> {
//! #         self.store.lock().unwrap().insert(cache_key, (res.clone(), policy));
//! #         Ok(res)
//! #     }
//! #     async fn delete(&self, cache_key: &str) -> http_cache::Result<()> {
//! #         self.store.lock().unwrap().remove(cache_key);
//! #         Ok(())
//! #     }
//! # }
//!
//! # tokio_test::block_on(async {
//! let manager = MemoryCacheManager::new();
//! let layer = ServerCacheLayer::new(manager);
//!
//! // Apply the layer to your Tower service
//! let service = tower::service_fn(|_req: Request<Full<Bytes>>| async {
//!     Ok::<_, std::io::Error>(
//!         Response::builder()
//!             .header("cache-control", "max-age=60")
//!             .body(Full::new(Bytes::from("Hello, World!")))
//!             .unwrap()
//!     )
//! });
//!
//! let mut cached_service = layer.layer(service);
//! # });
//! ```
//!
//! # Vary Header Support
//!
//! This cache enforces `Vary` headers using `http-cache-semantics`. When a response includes
//! a `Vary` header, subsequent requests must have matching header values to receive the cached
//! response. Requests with different header values will result in cache misses.
//!
//! For example, if a response has `Vary: Accept-Language`, a cached English response won't be
//! served to a request with `Accept-Language: de`.
//!
//! # Security Warnings
//!
//! This is a **shared cache** - cached responses are served to ALL users. Improper configuration
//! can leak user-specific data between different users.
//!
//! ## Authorization and Authentication
//!
//! This cache does not check for `Authorization` headers or session cookies in requests.
//! Caching authenticated endpoints without proper cache key differentiation will cause
//! user A's response to be served to user B.
//!
//! **Do NOT cache authenticated endpoints** unless you use a `CustomKeyer` that includes
//! the user or session identifier in the cache key:
//!
//! ```rust
//! # use http_cache_tower_server::CustomKeyer;
//! # use http::Request;
//! // Example: Include session ID in cache key
//! let keyer = CustomKeyer::new(|req: &Request<()>| {
//!     let session = req.headers()
//!         .get("cookie")
//!         .and_then(|v| v.to_str().ok())
//!         .and_then(|c| extract_session_id(c))
//!         .unwrap_or("anonymous");
//!     format!("{} {} session:{}", req.method(), req.uri().path(), session)
//! });
//! # fn extract_session_id(cookie: &str) -> Option<&str> { None }
//! ```
//!
//! ## General Security Considerations
//!
//! - Never cache responses containing user-specific data without user-specific cache keys
//! - Validate cache keys to prevent cache poisoning attacks
//! - Be careful with header-based caching due to header injection risks
//! - Consider the `private` Cache-Control directive for user-specific responses (automatically rejected by this cache)

#![warn(missing_docs)]
#![deny(unsafe_code)]

use bytes::Bytes;
use http::{header::HeaderValue, Request, Response};
use http_body::{Body as HttpBody, Frame};
use http_body_util::BodyExt;
use http_cache::{CacheManager, HttpResponse, HttpVersion};
use http_cache_semantics::{BeforeRequest, CachePolicy};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime};
use tower::{Layer, Service};

type BoxError = Box<dyn StdError + Send + Sync>;

/// Cache performance metrics.
///
/// Tracks hits, misses, and stores for monitoring cache effectiveness.
#[derive(Debug, Default)]
pub struct CacheMetrics {
    /// Number of cache hits.
    pub hits: AtomicU64,
    /// Number of cache misses.
    pub misses: AtomicU64,
    /// Number of responses stored in cache.
    pub stores: AtomicU64,
    /// Number of responses skipped (too large, not cacheable, etc.).
    pub skipped: AtomicU64,
}

impl CacheMetrics {
    /// Create new metrics instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate cache hit rate as a percentage (0.0 to 1.0).
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let total = hits + self.misses.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Reset all metrics to zero.
    pub fn reset(&self) {
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.stores.store(0, Ordering::Relaxed);
        self.skipped.store(0, Ordering::Relaxed);
    }
}

/// A trait for generating cache keys from HTTP requests.
pub trait Keyer: Clone + Send + Sync + 'static {
    /// Generate a cache key for the given request.
    fn cache_key<B>(&self, req: &Request<B>) -> String;
}

/// Default keyer that uses HTTP method and path.
///
/// Generates keys in the format: `{METHOD} {path}`
///
/// # Example
///
/// ```
/// # use http::Request;
/// # use http_cache_tower_server::{Keyer, DefaultKeyer};
/// let keyer = DefaultKeyer;
/// let req = Request::get("/users/123").body(()).unwrap();
/// let key = keyer.cache_key(&req);
/// assert_eq!(key, "GET /users/123");
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultKeyer;

impl Keyer for DefaultKeyer {
    fn cache_key<B>(&self, req: &Request<B>) -> String {
        format!("{} {}", req.method(), req.uri().path())
    }
}

/// Keyer that includes query parameters in the cache key.
///
/// Generates keys in the format: `{METHOD} {path}?{query}`
///
/// # Example
///
/// ```
/// # use http::Request;
/// # use http_cache_tower_server::{Keyer, QueryKeyer};
/// let keyer = QueryKeyer;
/// let req = Request::get("/users?page=1").body(()).unwrap();
/// let key = keyer.cache_key(&req);
/// assert_eq!(key, "GET /users?page=1");
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct QueryKeyer;

impl Keyer for QueryKeyer {
    fn cache_key<B>(&self, req: &Request<B>) -> String {
        format!("{} {}", req.method(), req.uri())
    }
}

/// Custom keyer that uses a user-provided function.
///
/// Use this when the default method+path keying is insufficient, such as:
/// - Content negotiation based on request headers (Accept-Language, Accept-Encoding)
/// - User-specific or session-specific caching
/// - Query parameter normalization
///
/// # Examples
///
/// Basic custom format:
///
/// ```
/// # use http::Request;
/// # use http_cache_tower_server::{Keyer, CustomKeyer};
/// let keyer = CustomKeyer::new(|req: &Request<()>| {
///     format!("custom-{}-{}", req.method(), req.uri().path())
/// });
/// let req = Request::get("/users").body(()).unwrap();
/// let key = keyer.cache_key(&req);
/// assert_eq!(key, "custom-GET-/users");
/// ```
///
/// Content negotiation (Accept-Language):
///
/// ```
/// # use http::Request;
/// # use http_cache_tower_server::{Keyer, CustomKeyer};
/// let keyer = CustomKeyer::new(|req: &Request<()>| {
///     let lang = req.headers()
///         .get("accept-language")
///         .and_then(|v| v.to_str().ok())
///         .and_then(|s| s.split(',').next())
///         .unwrap_or("en");
///     format!("{} {} lang:{}", req.method(), req.uri().path(), lang)
/// });
/// ```
///
/// User-specific caching (session-based):
///
/// ```
/// # use http::Request;
/// # use http_cache_tower_server::{Keyer, CustomKeyer};
/// let keyer = CustomKeyer::new(|req: &Request<()>| {
///     let user_id = req.headers()
///         .get("x-user-id")
///         .and_then(|v| v.to_str().ok())
///         .unwrap_or("anonymous");
///     format!("{} {} user:{}", req.method(), req.uri().path(), user_id)
/// });
/// ```
///
/// # Security Warning
///
/// When caching user-specific or session-specific data, ensure the user/session identifier
/// is included in the cache key. Failure to do so will cause responses from one user to be
/// served to other users.
#[derive(Clone)]
pub struct CustomKeyer<F> {
    func: F,
}

impl<F> CustomKeyer<F> {
    /// Create a new custom keyer with the given function.
    pub fn new(func: F) -> Self {
        Self { func }
    }
}

impl<F> Keyer for CustomKeyer<F>
where
    F: Fn(&Request<()>) -> String + Clone + Send + Sync + 'static,
{
    fn cache_key<B>(&self, req: &Request<B>) -> String {
        // Create a temporary request with the same parts but () body
        let mut temp_req = Request::builder()
            .method(req.method())
            .uri(req.uri())
            .version(req.version())
            .body(())
            .unwrap();

        // Copy headers for content negotiation support
        *temp_req.headers_mut() = req.headers().clone();

        (self.func)(&temp_req)
    }
}

/// Configuration options for server-side caching.
#[derive(Debug, Clone)]
pub struct ServerCacheOptions {
    /// Default TTL when response has no Cache-Control header.
    pub default_ttl: Option<Duration>,

    /// Maximum TTL, even if response specifies longer.
    pub max_ttl: Option<Duration>,

    /// Minimum TTL, even if response specifies shorter.
    pub min_ttl: Option<Duration>,

    /// Whether to add X-Cache headers (HIT/MISS).
    pub cache_status_headers: bool,

    /// Maximum response body size to cache (in bytes).
    pub max_body_size: usize,

    /// Whether to cache responses without explicit Cache-Control.
    pub cache_by_default: bool,

    /// Whether to respect Vary header for content negotiation.
    ///
    /// When true (default), cached responses are only served if the request's
    /// headers match those specified in the response's Vary header. This is
    /// enforced via `http-cache-semantics`.
    pub respect_vary: bool,

    /// Whether to respect Authorization headers per RFC 9111 §3.5.
    ///
    /// When true (default), requests with `Authorization` headers are not cached
    /// unless the response explicitly permits it via `public`, `s-maxage`, or
    /// `must-revalidate` directives.
    ///
    /// This prevents accidental caching of authenticated responses that could
    /// leak user-specific data to other users.
    pub respect_authorization: bool,
}

impl Default for ServerCacheOptions {
    fn default() -> Self {
        Self {
            default_ttl: Some(Duration::from_secs(60)),
            max_ttl: Some(Duration::from_secs(3600)),
            min_ttl: None,
            cache_status_headers: true,
            max_body_size: 128 * 1024 * 1024,
            cache_by_default: false,
            respect_vary: true,
            respect_authorization: true,
        }
    }
}

/// A cached HTTP response with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResponse {
    /// Response status code.
    pub status: u16,

    /// Response headers.
    pub headers: HashMap<String, String>,

    /// Response body bytes.
    pub body: Vec<u8>,

    /// When this response was cached.
    pub cached_at: SystemTime,

    /// Time-to-live duration.
    pub ttl: Duration,

    /// Optional vary headers for content negotiation.
    pub vary: Option<Vec<String>>,
}

impl CachedResponse {
    /// Check if this cached response is stale.
    pub fn is_stale(&self) -> bool {
        SystemTime::now()
            .duration_since(self.cached_at)
            .unwrap_or(Duration::MAX)
            > self.ttl
    }

    /// Convert to an HTTP response.
    pub fn into_response(self) -> Response<Bytes> {
        let mut builder = Response::builder().status(self.status);

        for (key, value) in self.headers {
            if let Ok(header_value) = HeaderValue::from_str(&value) {
                builder = builder.header(key, header_value);
            }
        }

        builder.body(Bytes::from(self.body)).unwrap()
    }
}

/// Response body types.
#[derive(Debug)]
pub enum ResponseBody {
    /// Cached response body.
    Cached(Bytes),
    /// Fresh response body.
    Fresh(Bytes),
    /// Uncacheable response body.
    Uncacheable(Bytes),
}

impl HttpBody for ResponseBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Self::Data>, Self::Error>>> {
        let bytes = match &mut *self {
            ResponseBody::Cached(b)
            | ResponseBody::Fresh(b)
            | ResponseBody::Uncacheable(b) => {
                std::mem::replace(b, Bytes::new())
            }
        };

        if bytes.is_empty() {
            Poll::Ready(None)
        } else {
            Poll::Ready(Some(Ok(Frame::data(bytes))))
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            ResponseBody::Cached(b)
            | ResponseBody::Fresh(b)
            | ResponseBody::Uncacheable(b) => b.is_empty(),
        }
    }
}

/// Tower layer for server-side HTTP response caching.
///
/// This layer should be placed AFTER routing to ensure request
/// extensions (like path parameters) are preserved.
///
/// # Shared Cache Behavior
///
/// This implements a **shared cache** as defined in RFC 9111. Responses cached by this layer
/// are served to all users making requests with matching cache keys. The cache automatically
/// rejects responses with the `private` directive, but does not inspect `Authorization` headers
/// or session cookies.
///
/// For authenticated or user-specific endpoints, either:
/// - Set `Cache-Control: private` in responses (prevents caching)
/// - Use a `CustomKeyer` that includes user/session identifiers in the cache key
#[derive(Clone)]
pub struct ServerCacheLayer<M, K = DefaultKeyer>
where
    M: CacheManager,
    K: Keyer,
{
    manager: M,
    keyer: K,
    options: ServerCacheOptions,
    metrics: Arc<CacheMetrics>,
}

impl<M> ServerCacheLayer<M, DefaultKeyer>
where
    M: CacheManager,
{
    /// Create a new cache layer with default options.
    pub fn new(manager: M) -> Self {
        Self {
            manager,
            keyer: DefaultKeyer,
            options: ServerCacheOptions::default(),
            metrics: Arc::new(CacheMetrics::new()),
        }
    }
}

impl<M, K> ServerCacheLayer<M, K>
where
    M: CacheManager,
    K: Keyer,
{
    /// Create a cache layer with a custom keyer.
    pub fn with_keyer(manager: M, keyer: K) -> Self {
        Self {
            manager,
            keyer,
            options: ServerCacheOptions::default(),
            metrics: Arc::new(CacheMetrics::new()),
        }
    }

    /// Set custom options.
    pub fn with_options(mut self, options: ServerCacheOptions) -> Self {
        self.options = options;
        self
    }

    /// Get a reference to the cache metrics.
    pub fn metrics(&self) -> &Arc<CacheMetrics> {
        &self.metrics
    }

    /// Invalidate a specific cache entry by its key.
    pub async fn invalidate(&self, cache_key: &str) -> Result<(), BoxError> {
        self.manager.delete(cache_key).await
    }

    /// Invalidate cache entry for a specific request.
    ///
    /// Uses the configured keyer to generate the cache key from the request.
    pub async fn invalidate_request<B>(
        &self,
        req: &Request<B>,
    ) -> Result<(), BoxError> {
        let cache_key = self.keyer.cache_key(req);
        self.invalidate(&cache_key).await
    }
}

impl<S, M, K> Layer<S> for ServerCacheLayer<M, K>
where
    M: CacheManager + Clone,
    K: Keyer,
{
    type Service = ServerCacheService<S, M, K>;

    fn layer(&self, inner: S) -> Self::Service {
        ServerCacheService {
            inner,
            manager: self.manager.clone(),
            keyer: self.keyer.clone(),
            options: self.options.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

/// Tower service that implements response caching.
#[derive(Clone)]
pub struct ServerCacheService<S, M, K>
where
    M: CacheManager,
    K: Keyer,
{
    inner: S,
    manager: M,
    keyer: K,
    options: ServerCacheOptions,
    metrics: Arc<CacheMetrics>,
}

impl<S, ReqBody, ResBody, M, K> Service<Request<ReqBody>>
    for ServerCacheService<S, M, K>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Error: Into<BoxError>,
    S::Future: Send + 'static,
    M: CacheManager + Clone,
    K: Keyer,
    ReqBody: Send + 'static,
    ResBody: HttpBody + Send + 'static,
    ResBody::Data: Send,
    ResBody::Error: Into<BoxError>,
{
    type Response = Response<ResponseBody>;
    type Error = BoxError;
    type Future = Pin<
        Box<
            dyn std::future::Future<
                    Output = std::result::Result<Self::Response, Self::Error>,
                > + Send,
        >,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let manager = self.manager.clone();
        let keyer = self.keyer.clone();
        let options = self.options.clone();
        let metrics = self.metrics.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Store request parts for later use in should_cache
            let (req_parts, req_body) = req.into_parts();

            // Generate cache key from request parts
            let temp_req = Request::from_parts(req_parts.clone(), ());
            let cache_key = keyer.cache_key(&temp_req);

            // Try to get from cache
            if let Ok(Some((cached_resp, policy))) =
                manager.get(&cache_key).await
            {
                // Deserialize cached response first
                if let Ok(cached) =
                    serde_json::from_slice::<CachedResponse>(&cached_resp.body)
                {
                    // Check freshness using both CachePolicy and our TTL tracking.
                    // CachePolicy handles Vary header matching.
                    // Our is_stale() handles the TTL we assigned (especially for cache_by_default).
                    let before_req =
                        policy.before_request(&req_parts, SystemTime::now());

                    // Determine if response had explicit freshness directives
                    // (max-age or s-maxage). If it only has "public" or other directives
                    // without explicit TTL, we use our own TTL tracking.
                    let has_explicit_ttl =
                        cached.headers.get("cache-control").is_some_and(|cc| {
                            cc.contains("max-age") || cc.contains("s-maxage")
                        });

                    let is_fresh = match before_req {
                        BeforeRequest::Fresh(_) => {
                            // CachePolicy says fresh - use it
                            true
                        }
                        BeforeRequest::Stale { .. } => {
                            // CachePolicy says stale. This could be due to:
                            // 1. Vary header mismatch
                            // 2. Time-based staleness per cache headers
                            // 3. No explicit TTL (cache_by_default or public-only)
                            //
                            // For case 3, our TTL tracking is authoritative.
                            // For cases 1-2, we should respect CachePolicy.
                            if has_explicit_ttl {
                                // Had explicit TTL - trust CachePolicy
                                false
                            } else {
                                // No explicit TTL - use our TTL
                                !cached.is_stale()
                            }
                        }
                    };

                    if is_fresh {
                        // Cache hit
                        metrics.hits.fetch_add(1, Ordering::Relaxed);
                        let mut response = cached.into_response();

                        if options.cache_status_headers {
                            response.headers_mut().insert(
                                "x-cache",
                                HeaderValue::from_static("HIT"),
                            );
                        }

                        return Ok(response.map(ResponseBody::Cached));
                    }
                }
            }

            // Reconstruct request for handler
            let req = Request::from_parts(req_parts.clone(), req_body);

            // Cache miss or stale - call the handler
            metrics.misses.fetch_add(1, Ordering::Relaxed);
            let response = inner.call(req).await.map_err(Into::into)?;

            // Split response to check if we should cache
            let (res_parts, body) = response.into_parts();

            // Check if we should cache this response
            if let Some(ttl) = should_cache(&req_parts, &res_parts, &options) {
                // Buffer the response body
                let body_bytes = match collect_body(body).await {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        // If we can't collect the body, return an error response
                        return Err(e);
                    }
                };

                // Check size limit
                if body_bytes.len() <= options.max_body_size {
                    metrics.stores.fetch_add(1, Ordering::Relaxed);
                    // Create cached response
                    let cached = CachedResponse {
                        status: res_parts.status.as_u16(),
                        headers: res_parts
                            .headers
                            .iter()
                            .filter_map(|(k, v)| {
                                v.to_str()
                                    .ok()
                                    .map(|s| (k.to_string(), s.to_string()))
                            })
                            .collect(),
                        body: body_bytes.to_vec(),
                        cached_at: SystemTime::now(),
                        ttl,
                        vary: extract_vary_headers(&res_parts),
                    };

                    // Store in cache (fire and forget)
                    let cached_json = serde_json::to_vec(&cached)
                        .map_err(|e| Box::new(e) as BoxError)?;
                    let http_response = HttpResponse {
                        body: cached_json,
                        headers: Default::default(),
                        status: 200,
                        url: cache_key.clone().parse().unwrap_or_else(|_| {
                            "http://localhost/".parse().unwrap()
                        }),
                        version: HttpVersion::Http11,
                        metadata: None,
                    };

                    // Create CachePolicy from actual request/response for Vary support
                    let policy_req = Request::from_parts(req_parts.clone(), ());
                    let policy_res =
                        Response::from_parts(res_parts.clone(), ());
                    let policy = CachePolicy::new(&policy_req, &policy_res);

                    // Spawn cache write asynchronously
                    let manager_clone = manager.clone();
                    tokio::spawn(async move {
                        let _ = manager_clone
                            .put(cache_key, http_response, policy)
                            .await;
                    });
                } else {
                    // Body too large
                    metrics.skipped.fetch_add(1, Ordering::Relaxed);
                }

                // Return response with MISS header
                let mut response = Response::from_parts(res_parts, body_bytes);
                if options.cache_status_headers {
                    response
                        .headers_mut()
                        .insert("x-cache", HeaderValue::from_static("MISS"));
                }
                return Ok(response.map(ResponseBody::Fresh));
            }

            // Don't cache - just return
            metrics.skipped.fetch_add(1, Ordering::Relaxed);
            let body_bytes = collect_body(body).await?;
            Ok(Response::from_parts(res_parts, body_bytes)
                .map(ResponseBody::Uncacheable))
        })
    }
}

/// Collect a body into bytes.
async fn collect_body<B>(body: B) -> std::result::Result<Bytes, BoxError>
where
    B: HttpBody,
    B::Error: Into<BoxError>,
{
    body.collect()
        .await
        .map(|collected| collected.to_bytes())
        .map_err(Into::into)
}

/// Extract Vary headers from response parts.
fn extract_vary_headers(parts: &http::response::Parts) -> Option<Vec<String>> {
    parts
        .headers
        .get(http::header::VARY)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').map(|h| h.trim().to_string()).collect())
}

/// Determine if a response should be cached based on its headers.
/// Implements RFC 7234/9111 requirements for shared caches.
/// Helper function to check if a Cache-Control directive is present.
/// This properly parses directives by splitting on commas and matching exact names.
fn has_directive(cache_control: &str, directive: &str) -> bool {
    cache_control
        .split(',')
        .map(|d| d.trim())
        .any(|d| d == directive || d.starts_with(&format!("{}=", directive)))
}

/// Check if response explicitly permits caching of authorized requests per RFC 9111 §3.5.
///
/// Returns true if the response contains directives that allow caching despite
/// the request having an Authorization header.
fn response_permits_authorized_caching(cc_str: &str) -> bool {
    has_directive(cc_str, "public")
        || has_directive(cc_str, "s-maxage")
        || has_directive(cc_str, "must-revalidate")
}

fn should_cache(
    req_parts: &http::request::Parts,
    res_parts: &http::response::Parts,
    options: &ServerCacheOptions,
) -> Option<Duration> {
    // RFC 7234: Only cache successful responses (2xx)
    if !res_parts.status.is_success() {
        return None;
    }

    // RFC 9111 §3.5: Check Authorization header
    let has_authorization =
        req_parts.headers.contains_key(http::header::AUTHORIZATION);

    // RFC 7234: Check Cache-Control directives
    if let Some(cc) = res_parts.headers.get(http::header::CACHE_CONTROL) {
        let cc_str = cc.to_str().ok()?;

        // RFC 9111 §3.5: If request has Authorization header, only cache if
        // response explicitly permits it
        if has_authorization
            && options.respect_authorization
            && !response_permits_authorized_caching(cc_str)
        {
            return None;
        }

        // RFC 7234: MUST NOT store if no-store directive present
        if has_directive(cc_str, "no-store") {
            return None;
        }

        // RFC 7234: MUST NOT store if no-cache
        // Note: Per RFC, no-cache means "cache but always revalidate". However,
        // without conditional request support (ETag/If-None-Match), we cannot
        // revalidate, so we skip caching entirely.
        if has_directive(cc_str, "no-cache") {
            return None;
        }

        // RFC 7234: Shared caches MUST NOT store responses with private directive
        if has_directive(cc_str, "private") {
            return None;
        }

        // RFC 7234: s-maxage directive overrides max-age for shared caches
        if let Some(s_maxage) = parse_s_maxage(cc_str) {
            let ttl = Duration::from_secs(s_maxage);
            let ttl = apply_ttl_constraints(ttl, options);
            return Some(ttl);
        }

        // RFC 7234: Extract max-age for cache lifetime
        if let Some(max_age) = parse_max_age(cc_str) {
            let ttl = Duration::from_secs(max_age);
            let ttl = apply_ttl_constraints(ttl, options);
            return Some(ttl);
        }

        // RFC 7234: public directive makes response cacheable
        if has_directive(cc_str, "public") {
            return options.default_ttl;
        }
    } else {
        // No Cache-Control header
        // RFC 9111 §3.5: Don't cache authorized requests without explicit permission
        if has_authorization && options.respect_authorization {
            return None;
        }
    }

    // RFC 7234: Check for Expires header if no Cache-Control
    if let Some(expires) = res_parts.headers.get(http::header::EXPIRES) {
        if let Ok(expires_str) = expires.to_str() {
            if let Some(ttl) = parse_expires(expires_str) {
                let ttl = apply_ttl_constraints(ttl, options);
                return Some(ttl);
            }
        }
    }

    // No explicit caching directive
    if options.cache_by_default {
        options.default_ttl
    } else {
        None
    }
}

/// Apply min/max TTL constraints from options.
fn apply_ttl_constraints(
    ttl: Duration,
    options: &ServerCacheOptions,
) -> Duration {
    let mut result = ttl;

    if let Some(max) = options.max_ttl {
        result = result.min(max);
    }

    if let Some(min) = options.min_ttl {
        result = result.max(min);
    }

    result
}

/// Parse max-age from Cache-Control header.
fn parse_max_age(cache_control: &str) -> Option<u64> {
    for directive in cache_control.split(',') {
        let directive = directive.trim();
        if let Some(value) = directive.strip_prefix("max-age=") {
            return value.parse().ok();
        }
    }
    None
}

/// Parse s-maxage from Cache-Control header (shared cache specific).
fn parse_s_maxage(cache_control: &str) -> Option<u64> {
    for directive in cache_control.split(',') {
        let directive = directive.trim();
        if let Some(value) = directive.strip_prefix("s-maxage=") {
            return value.parse().ok();
        }
    }
    None
}

/// Parse Expires header to calculate TTL.
///
/// Returns the duration until expiration, or None if the date is invalid or in the past.
fn parse_expires(expires: &str) -> Option<Duration> {
    let expires_time = httpdate::parse_http_date(expires).ok()?;
    let now = SystemTime::now();

    expires_time.duration_since(now).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_keyer() {
        let keyer = DefaultKeyer;
        let req = Request::get("/users/123").body(()).unwrap();
        let key = keyer.cache_key(&req);
        assert_eq!(key, "GET /users/123");
    }

    #[test]
    fn test_query_keyer() {
        let keyer = QueryKeyer;
        let req = Request::get("/users?page=1").body(()).unwrap();
        let key = keyer.cache_key(&req);
        assert_eq!(key, "GET /users?page=1");
    }

    #[test]
    fn test_parse_max_age() {
        assert_eq!(parse_max_age("max-age=3600"), Some(3600));
        assert_eq!(parse_max_age("public, max-age=3600"), Some(3600));
        assert_eq!(parse_max_age("max-age=3600, public"), Some(3600));
        assert_eq!(parse_max_age("public"), None);
    }

    #[test]
    fn test_parse_s_maxage() {
        assert_eq!(parse_s_maxage("s-maxage=7200"), Some(7200));
        assert_eq!(parse_s_maxage("public, s-maxage=7200"), Some(7200));
        assert_eq!(parse_s_maxage("s-maxage=7200, max-age=3600"), Some(7200));
        assert_eq!(parse_s_maxage("public"), None);
    }

    #[test]
    fn test_apply_ttl_constraints() {
        let options = ServerCacheOptions {
            min_ttl: Some(Duration::from_secs(10)),
            max_ttl: Some(Duration::from_secs(100)),
            ..Default::default()
        };

        assert_eq!(
            apply_ttl_constraints(Duration::from_secs(5), &options),
            Duration::from_secs(10)
        );
        assert_eq!(
            apply_ttl_constraints(Duration::from_secs(50), &options),
            Duration::from_secs(50)
        );
        assert_eq!(
            apply_ttl_constraints(Duration::from_secs(200), &options),
            Duration::from_secs(100)
        );
    }
}
