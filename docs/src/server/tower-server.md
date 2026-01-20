# tower-server

The [`http-cache-tower-server`](https://github.com/06chaynes/http-cache/tree/main/http-cache-tower-server) crate provides Tower Layer and Service implementations for server-side HTTP response caching. Unlike client-side caching, this middleware caches your own application's responses to reduce database queries, computation, and improve response times.

## Key Differences from Client-Side Caching

**Client-Side (`http-cache-tower`)**: Caches responses from external APIs you're calling
**Server-Side (`http-cache-tower-server`)**: Caches responses your application generates

**Critical:** Server-side cache middleware must be placed **AFTER** routing in your middleware stack to preserve request extensions like path parameters (see [Issue #121](https://github.com/06chaynes/http-cache/issues/121)).

## Getting Started

```sh
cargo add http-cache-tower-server
```

## Features

- `manager-cacache`: (default) Enables the [`CACacheManager`](https://docs.rs/http-cache/latest/http_cache/struct.CACacheManager.html) backend cache manager.
- `manager-moka`: Enables the [`MokaManager`](https://docs.rs/http-cache/latest/http_cache/struct.MokaManager.html) backend cache manager.

## Basic Usage with Axum

```rust
use axum::{
    routing::get,
    Router,
    extract::Path,
};
use http_cache_tower_server::ServerCacheLayer;
use http_cache::CACacheManager;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    // Create cache manager
    let cache_manager = CACacheManager::new(PathBuf::from("./cache"), false);

    // Create the server cache layer
    let cache_layer = ServerCacheLayer::new(cache_manager);

    // Build your Axum app
    let app = Router::new()
        .route("/users/:id", get(get_user))
        .route("/posts/:id", get(get_post))
        // IMPORTANT: Place cache layer AFTER routing
        .layer(cache_layer);

    // Run the server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_user(Path(id): Path<u32>) -> String {
    // Expensive database query or computation
    format!("User {}", id)
}

async fn get_post(Path(id): Path<u32>) -> String {
    format!("Post {}", id)
}
```

## Cache Control with Response Headers

The middleware respects standard HTTP Cache-Control headers from your handlers:

```rust
use axum::{
    response::{IntoResponse, Response},
    http::header,
};

async fn cacheable_handler() -> Response {
    (
        [(header::CACHE_CONTROL, "max-age=300")], // Cache for 5 minutes
        "This response will be cached"
    ).into_response()
}

async fn no_cache_handler() -> Response {
    (
        [(header::CACHE_CONTROL, "no-store")], // Don't cache
        "This response will NOT be cached"
    ).into_response()
}

async fn private_handler() -> Response {
    (
        [(header::CACHE_CONTROL, "private")], // User-specific data
        "This response will NOT be cached (shared cache)"
    ).into_response()
}
```

## RFC 7234 Compliance

This implementation acts as a **shared cache** per RFC 7234:

### Automatically Rejects

- `no-store` directive
- `no-cache` directive (requires revalidation, which is not supported)
- `private` directive (shared caches cannot store private responses)
- Non-2xx status codes

### Supports

- `max-age`: Cache lifetime in seconds
- `s-maxage`: Shared cache specific lifetime (takes precedence over max-age)
- `public`: Makes response cacheable
- `Expires`: Fallback header when Cache-Control is absent

## Cache Key Strategies

### DefaultKeyer (Default)

Caches based on HTTP method and path:

```rust
use http_cache_tower_server::{ServerCacheLayer, DefaultKeyer};

let cache_layer = ServerCacheLayer::new(cache_manager);
// GET /users/123 and GET /users/456 are cached separately
```

### QueryKeyer

Includes query parameters in the cache key:

```rust
use http_cache_tower_server::{ServerCacheLayer, QueryKeyer};

let cache_layer = ServerCacheLayer::with_keyer(cache_manager, QueryKeyer);
// GET /search?q=rust and GET /search?q=python are cached separately
```

### CustomKeyer

For advanced use cases like content negotiation or user-specific caching:

```rust
use http_cache_tower_server::{ServerCacheLayer, CustomKeyer};

// Example: Include Accept-Language header in cache key
let keyer = CustomKeyer::new(|req: &http::Request<()>| {
    let lang = req.headers()
        .get("accept-language")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("en");
    format!("{} {} lang:{}", req.method(), req.uri().path(), lang)
});

let cache_layer = ServerCacheLayer::with_keyer(cache_manager, keyer);
```

## Configuration Options

```rust
use http_cache_tower_server::{ServerCacheLayer, ServerCacheOptions};
use std::time::Duration;

let options = ServerCacheOptions {
    // Default TTL when no Cache-Control header present
    default_ttl: Some(Duration::from_secs(60)),

    // Maximum TTL (even if response specifies longer)
    max_ttl: Some(Duration::from_secs(3600)),

    // Minimum TTL (even if response specifies shorter)
    min_ttl: Some(Duration::from_secs(10)),

    // Add X-Cache: HIT/MISS headers for debugging
    cache_status_headers: true,

    // Maximum body size to cache (bytes)
    max_body_size: 128 * 1024 * 1024, // 128 MB

    // Cache responses without Cache-Control header
    cache_by_default: false,

    // Respect Vary header for content negotiation (default: true)
    // When enabled, cached responses are only served if the request's
    // headers match those specified in the response's Vary header
    respect_vary: true,

    // Respect Authorization headers per RFC 7234 (default: true)
    // When enabled, requests with Authorization headers are not cached
    // unless the response explicitly permits it via public, s-maxage,
    // or must-revalidate directives
    respect_authorization: true,
};

let cache_layer = ServerCacheLayer::new(cache_manager)
    .with_options(options);
```

## Security Warnings

### Shared Cache Behavior

This is a **shared cache** - cached responses are served to ALL users. Improper configuration can leak user-specific data.

### Do NOT Cache

- Authenticated endpoints (unless using appropriate CustomKeyer)
- User-specific data (unless keyed by user/session ID)
- Responses with sensitive information

### Safe Approaches

**Option 1: Use Cache-Control: private**

```rust
async fn user_specific_handler() -> Response {
    (
        [(header::CACHE_CONTROL, "private")],
        "User-specific data - won't be cached"
    ).into_response()
}
```

**Option 2: Include user ID in cache key**

```rust
let keyer = CustomKeyer::new(|req: &http::Request<()>| {
    let user_id = req.headers()
        .get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous");
    format!("{} {} user:{}", req.method(), req.uri().path(), user_id)
});
```

**Option 3: Don't cache at all**

```rust
async fn sensitive_handler() -> Response {
    (
        [(header::CACHE_CONTROL, "no-store")],
        "Sensitive data - never cached"
    ).into_response()
}
```

## Content Negotiation

The middleware respects `Vary` headers via `http-cache-semantics` when `respect_vary` is enabled (the default). Cached responses are only served if the request's headers match those specified in the response's `Vary` header. For additional control, you can also use a `CustomKeyer`:

```rust
// Example: Cache different responses based on Accept-Language
let keyer = CustomKeyer::new(|req: &http::Request<()>| {
    let lang = req.headers()
        .get("accept-language")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .unwrap_or("en");
    format!("{} {} lang:{}", req.method(), req.uri().path(), lang)
});
```

## Cache Inspection

Responses include `X-Cache` headers when `cache_status_headers` is enabled:

- `X-Cache: HIT` - Response served from cache
- `X-Cache: MISS` - Response generated by handler and cached (if cacheable)
- No header - Response not cacheable (or headers disabled)

## Cache Metrics

The `ServerCacheLayer` provides access to cache performance metrics via `CacheMetrics`:

```rust
use http_cache_tower_server::{ServerCacheLayer, CacheMetrics};
use std::sync::Arc;

// Create the cache layer
let cache_layer = ServerCacheLayer::new(cache_manager);

// Access metrics
let metrics: &Arc<CacheMetrics> = cache_layer.metrics();

// Get hit rate (0.0 to 1.0)
let hit_rate = metrics.hit_rate();
println!("Cache hit rate: {:.2}%", hit_rate * 100.0);

// Access individual counters
println!("Hits: {}", metrics.hits.load(std::sync::atomic::Ordering::Relaxed));
println!("Misses: {}", metrics.misses.load(std::sync::atomic::Ordering::Relaxed));
println!("Stores: {}", metrics.stores.load(std::sync::atomic::Ordering::Relaxed));
println!("Skipped: {}", metrics.skipped.load(std::sync::atomic::Ordering::Relaxed));

// Reset metrics
metrics.reset();
```

### Available Metrics

- `hits`: Number of cache hits
- `misses`: Number of cache misses
- `stores`: Number of responses stored in cache
- `skipped`: Number of responses skipped (too large, not cacheable, etc.)

## Complete Example

```rust
use axum::{
    routing::get,
    Router,
    extract::Path,
    response::{IntoResponse, Response},
    http::header,
};
use http_cache_tower_server::{ServerCacheLayer, ServerCacheOptions, QueryKeyer};
use http_cache::CACacheManager;
use std::time::Duration;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    // Configure cache manager
    let cache_manager = CACacheManager::new(PathBuf::from("./cache"), false);

    // Configure cache options
    let options = ServerCacheOptions {
        default_ttl: Some(Duration::from_secs(60)),
        max_ttl: Some(Duration::from_secs(3600)),
        cache_status_headers: true,
        ..Default::default()
    };

    // Create cache layer with query parameter support
    let cache_layer = ServerCacheLayer::with_keyer(cache_manager, QueryKeyer)
        .with_options(options);

    // Build app
    let app = Router::new()
        .route("/users/:id", get(get_user))
        .route("/search", get(search))
        .route("/admin/stats", get(admin_stats))
        .layer(cache_layer); // AFTER routing

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}

// Cacheable for 5 minutes
async fn get_user(Path(id): Path<u32>) -> Response {
    (
        [(header::CACHE_CONTROL, "max-age=300")],
        format!("User {}", id)
    ).into_response()
}

// Cacheable with query parameters
async fn search(query: axum::extract::Query<std::collections::HashMap<String, String>>) -> Response {
    (
        [(header::CACHE_CONTROL, "max-age=60")],
        format!("Search results: {:?}", query)
    ).into_response()
}

// Never cached (admin data)
async fn admin_stats() -> Response {
    (
        [(header::CACHE_CONTROL, "no-store")],
        "Admin statistics - not cached"
    ).into_response()
}
```

## Best Practices

1. **Place middleware after routing** to preserve request extensions
2. **Set appropriate Cache-Control headers** in your handlers
3. **Use `private` directive** for user-specific responses
4. **Monitor cache hit rates** using X-Cache headers
5. **Set reasonable TTL limits** to prevent stale data
6. **Use CustomKeyer** for content negotiation or user-specific caching
7. **Don't cache authenticated endpoints** without proper keying

## Troubleshooting

### Path parameters not working

**Problem:** Axum path extractors fail with cached responses

**Solution:** Ensure cache layer is placed AFTER routing:

```rust
// ❌ Wrong - cache layer before routing
let app = Router::new()
    .layer(cache_layer)  // Too early!
    .route("/users/:id", get(handler));

// ✅ Correct - cache layer after routing
let app = Router::new()
    .route("/users/:id", get(handler))
    .layer(cache_layer);  // After routing
```

### Responses not being cached

**Possible causes:**
1. Response has `no-store`, `no-cache`, or `private` directive
2. Response is not 2xx status code
3. Response body exceeds `max_body_size`
4. `cache_by_default` is false and no Cache-Control header present

**Solution:** Add appropriate Cache-Control headers:

```rust
async fn handler() -> Response {
    (
        [(header::CACHE_CONTROL, "max-age=300")],
        "Response body"
    ).into_response()
}
```

### User data leaking between requests

**Problem:** Cached user-specific responses served to other users

**Solution:** Use `CustomKeyer` with user identifier:

```rust
let keyer = CustomKeyer::new(|req: &http::Request<()>| {
    let user = req.headers()
        .get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous");
    format!("{} {} user:{}", req.method(), req.uri().path(), user)
});
```

Or use `Cache-Control: private` to prevent caching entirely.

## Performance Considerations

- Cache writes are fire-and-forget (non-blocking)
- Cache lookups are async but fast (especially with in-memory managers)
- Body buffering is required (responses are fully buffered before caching)
- Consider using moka manager for frequently accessed data
- Use cacache manager for larger datasets with disk persistence

## Comparison with Other Frameworks

| Feature | http-cache-tower-server | Django Cache | NGINX FastCGI |
|---------|------------------------|--------------|---------------|
| Middleware-based | ✅ | ✅ | ❌ |
| RFC 7234 compliant | ✅ | ⚠️ Partial | ⚠️ Partial |
| Pluggable backends | ✅ | ✅ | ❌ |
| Custom cache keys | ✅ | ✅ | ✅ |
| Type-safe | ✅ | ❌ | ❌ |
| Async-first | ✅ | ❌ | ✅ |
