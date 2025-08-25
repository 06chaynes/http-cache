# Cache Modes

When constructing a new instance of `HttpCache`, you must specify a cache mode. The cache mode determines how the cache will behave in certain situations. These modes are similar to [make-fetch-happen cache options](https://github.com/npm/make-fetch-happen#--optscache). The available cache modes are:

- `Default`: This mode will inspect the HTTP cache on the way to the network. If there is a fresh response it will be used. If there is a stale response a conditional request will be created, and a normal request otherwise. It then updates the HTTP cache with the response. If the revalidation request fails (for example, on a 500 or if you're offline), the stale response will be returned.

- `NoStore`: This mode will ignore the HTTP cache on the way to the network. It will always create a normal request, and will never cache the response.

- `Reload`: This mode will ignore the HTTP cache on the way to the network. It will always create a normal request, and will update the HTTP cache with the response.

- `NoCache`: This mode will create a conditional request if there is a response in the HTTP cache and a normal request otherwise. It then updates the HTTP cache with the response.

- `ForceCache`: This mode will inspect the HTTP cache on the way to the network. If there is a cached response it will be used regardless of freshness. If there is no cached response it will create a normal request, and will update the cache with the response.

- `OnlyIfCached`: This mode will inspect the HTTP cache on the way to the network. If there is a cached response it will be used regardless of freshness. If there is no cached response it will return a `504 Gateway Timeout` error.

- `IgnoreRules`: This mode will ignore the HTTP headers and always store a response given it was a 200 status code. It will also ignore the staleness when retrieving a response from the cache, so expiration of the cached response will need to be handled manually. If there was no cached response it will create a normal request, and will update the cache with the response.

## Maximum TTL Control

When using cache modes like `IgnoreRules` that bypass server cache headers, you can use the `max_ttl` option to provide expiration control. This is particularly useful for preventing cached responses from persisting indefinitely.

### Usage

The `max_ttl` option accepts a `Duration` and sets a maximum time-to-live for cached responses:

```rust
use http_cache::{HttpCacheOptions, CACacheManager, HttpCache, CacheMode};
use std::time::Duration;

let manager = CACacheManager::new("./cache".into(), true);

let options = HttpCacheOptions {
    max_ttl: Some(Duration::from_secs(300)), // 5 minutes maximum
    ..Default::default()
};

let cache = HttpCache {
    mode: CacheMode::IgnoreRules, // Ignore server cache headers
    manager,
    options,
};
```

### Behavior

- **Override longer durations**: If the server specifies a longer cache duration (e.g., `max-age=3600`), `max_ttl` will reduce it to the specified limit
- **Respect shorter durations**: If the server specifies a shorter duration (e.g., `max-age=60`), the server's shorter duration will be used
- **Provide fallback duration**: When using `IgnoreRules` mode where server headers are ignored, `max_ttl` provides the cache duration

### Examples

**With IgnoreRules mode:**
```rust
// Cache everything for 5 minutes, ignoring server headers
let options = HttpCacheOptions {
    max_ttl: Some(Duration::from_secs(300)),
    ..Default::default()
};
let cache = HttpCache {
    mode: CacheMode::IgnoreRules,
    manager,
    options,
};
```

**With Default mode:**
```rust
// Respect server headers but limit cache duration to 1 hour maximum
let options = HttpCacheOptions {
    max_ttl: Some(Duration::from_hours(1)),
    ..Default::default()
};
let cache = HttpCache {
    mode: CacheMode::Default,
    manager,
    options,
};
```

## Content-Type Based Caching

You can implement selective caching based on response content types using the `response_cache_mode_fn` option. This allows you to cache only certain types of content while avoiding others.

### Basic Content-Type Filtering

```rust
use http_cache::{HttpCacheOptions, CACacheManager, HttpCache, CacheMode};
use std::sync::Arc;

let manager = CACacheManager::new("./cache".into(), true);

let options = HttpCacheOptions {
    response_cache_mode_fn: Some(Arc::new(|_request_parts, response| {
        // Check the Content-Type header to decide caching behavior
        if let Some(content_type) = response.headers.get("content-type") {
            match content_type.to_str().unwrap_or("") {
                // Cache JSON APIs aggressively (ignore no-cache headers)
                ct if ct.starts_with("application/json") => Some(CacheMode::ForceCache),
                // Cache images with default HTTP caching rules
                ct if ct.starts_with("image/") => Some(CacheMode::Default),
                // Cache static assets aggressively
                ct if ct.starts_with("text/css") => Some(CacheMode::ForceCache),
                ct if ct.starts_with("application/javascript") => Some(CacheMode::ForceCache),
                // Don't cache HTML pages (often dynamic)
                ct if ct.starts_with("text/html") => Some(CacheMode::NoStore),
                // Don't cache unknown content types
                _ => Some(CacheMode::NoStore),
            }
        } else {
            // No Content-Type header - don't cache for safety
            Some(CacheMode::NoStore)
        }
    })),
    ..Default::default()
};

let cache = HttpCache {
    mode: CacheMode::Default, // This gets overridden by response_cache_mode_fn
    manager,
    options,
};
```

### Advanced Content-Type Strategies

For more complex scenarios, you can combine content-type checking with other response properties:

```rust
use http_cache::{HttpCacheOptions, CACacheManager, HttpCache, CacheMode};
use std::sync::Arc;
use std::time::Duration;

let manager = CACacheManager::new("./cache".into(), true);

let options = HttpCacheOptions {
    response_cache_mode_fn: Some(Arc::new(|request_parts, response| {
        // Get content type
        let content_type = response.headers
            .get("content-type")
            .and_then(|ct| ct.to_str().ok())
            .unwrap_or("");

        // Get URL path for additional context
        let path = request_parts.uri.path();

        match content_type {
            // API responses
            ct if ct.starts_with("application/json") => {
                if path.starts_with("/api/") {
                    // Cache API responses, but respect server headers
                    Some(CacheMode::Default)
                } else {
                    // Force cache non-API JSON (like config files)
                    Some(CacheMode::ForceCache)
                }
            },
            // Static assets
            ct if ct.starts_with("text/css") || 
                  ct.starts_with("application/javascript") => {
                Some(CacheMode::ForceCache)
            },
            // Images
            ct if ct.starts_with("image/") => {
                if response.status == 200 {
                    Some(CacheMode::ForceCache)
                } else {
                    Some(CacheMode::NoStore) // Don't cache error images
                }
            },
            // HTML
            ct if ct.starts_with("text/html") => {
                if path.starts_with("/static/") {
                    Some(CacheMode::Default) // Static HTML can be cached
                } else {
                    Some(CacheMode::NoStore) // Dynamic HTML shouldn't be cached
                }
            },
            // Everything else
            _ => Some(CacheMode::NoStore),
        }
    })),
    // Limit cache duration to 1 hour max
    max_ttl: Some(Duration::from_secs(3600)),
    ..Default::default()
};

let cache = HttpCache {
    mode: CacheMode::Default,
    manager,
    options,
};
```

### Common Content-Type Patterns

Here are some common content-type based caching strategies:

**Static Assets (Aggressive Caching):**
- `text/css` - CSS stylesheets
- `application/javascript` - JavaScript files  
- `image/*` - All image types
- `font/*` - Web fonts

**API Responses (Conditional Caching):**
- `application/json` - JSON APIs
- `application/xml` - XML APIs
- `text/plain` - Plain text responses

**Dynamic Content (No Caching):**
- `text/html` - HTML pages (usually dynamic)
- `application/x-www-form-urlencoded` - Form submissions

### Combining with Other Options

Content-type based caching works well with other cache options:

```rust
use http_cache::{HttpCacheOptions, CACacheManager, HttpCache, CacheMode};
use std::sync::Arc;
use std::time::Duration;

let options = HttpCacheOptions {
    // Content-type based mode selection
    response_cache_mode_fn: Some(Arc::new(|_req, response| {
        match response.headers.get("content-type")?.to_str().ok()? {
            ct if ct.starts_with("application/json") => Some(CacheMode::ForceCache),
            ct if ct.starts_with("image/") => Some(CacheMode::Default),
            _ => Some(CacheMode::NoStore),
        }
    })),
    // Custom cache keys for better organization
    cache_key: Some(Arc::new(|req| {
        format!("{}:{}:{}", req.method, req.uri.host().unwrap_or(""), req.uri.path())
    })),
    // Maximum cache duration
    max_ttl: Some(Duration::from_secs(1800)), // 30 minutes
    // Add cache status headers for debugging
    cache_status_headers: true,
    ..Default::default()
};
```

This approach gives you fine-grained control over what gets cached based on the actual content type returned by the server.

## Complete Per-Request Customization

The HTTP cache library provides comprehensive per-request customization capabilities for cache keys, cache options, and cache modes. Here's a complete example showing all features:

```rust
use http_cache::{HttpCacheOptions, CACacheManager, HttpCache, CacheMode};
use std::sync::Arc;
use std::time::Duration;

let manager = CACacheManager::new("./cache".into(), true);

let options = HttpCacheOptions {
    // 1. Configure cache keys when initializing (per-request cache key override)
    cache_key: Some(Arc::new(|req: &http::request::Parts| {
        // Generate different cache keys based on request properties
        let path = req.uri.path();
        let query = req.uri.query().unwrap_or("");
        
        match path {
            // API endpoints: include user context in cache key
            p if p.starts_with("/api/") => {
                if let Some(auth) = req.headers.get("authorization") {
                    format!("api:{}:{}:{}:authenticated", req.method, path, query)
                } else {
                    format!("api:{}:{}:{}:anonymous", req.method, path, query)
                }
            },
            // Static assets: simple cache key
            p if p.starts_with("/static/") => {
                format!("static:{}:{}", req.method, req.uri)
            },
            // Dynamic pages: include important headers
            _ => {
                let accept_lang = req.headers.get("accept-language")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("en");
                format!("page:{}:{}:{}:{}", req.method, path, query, accept_lang)
            }
        }
    })),
    
    // 2. Override cache options on a per-request basis (request-based cache mode)
    cache_mode_fn: Some(Arc::new(|req: &http::request::Parts| {
        let path = req.uri.path();
        
        // Admin endpoints: never cache
        if path.starts_with("/admin/") {
            return CacheMode::NoStore;
        }
        
        // Check for cache control headers from client
        if req.headers.contains_key("x-no-cache") {
            return CacheMode::NoStore;
        }
        
        // Development mode: bypass cache
        if req.headers.get("x-env").and_then(|h| h.to_str().ok()) == Some("development") {
            return CacheMode::Reload;
        }
        
        // Static assets: force cache
        if path.starts_with("/static/") || path.ends_with(".css") || path.ends_with(".js") {
            return CacheMode::ForceCache;
        }
        
        // Default behavior for everything else
        CacheMode::Default
    })),
    
    // 3. Additional per-response cache override (response-based cache mode)
    response_cache_mode_fn: Some(Arc::new(|req: &http::request::Parts, response| {
        // Override cache behavior based on response content and status
        
        // Never cache error responses
        if response.status >= 400 {
            return Some(CacheMode::NoStore);
        }
        
        // Content-type based caching
        if let Some(content_type) = response.headers.get("content-type") {
            match content_type.to_str().unwrap_or("") {
                // Force cache JSON APIs even with no-cache headers
                ct if ct.starts_with("application/json") => Some(CacheMode::ForceCache),
                // Don't cache HTML in development
                ct if ct.starts_with("text/html") => {
                    if req.headers.get("x-env").and_then(|h| h.to_str().ok()) == Some("development") {
                        Some(CacheMode::NoStore)
                    } else {
                        None // Use default behavior
                    }
                },
                _ => None,
            }
        } else {
            None
        }
    })),
    
    // Cache busting for related resources
    cache_bust: Some(Arc::new(|req: &http::request::Parts, _cache_key_fn, current_key| {
        let path = req.uri.path();
        
        // When updating user data, bust user-specific caches
        if req.method == "POST" && path.starts_with("/api/users/") {
            if let Some(user_id) = path.strip_prefix("/api/users/").and_then(|s| s.split('/').next()) {
                return vec![
                    format!("api:GET:/api/users/{}:authenticated", user_id),
                    format!("api:GET:/api/users/{}:anonymous", user_id),
                    format!("api:GET:/api/users:authenticated"),
                ];
            }
        }
        
        vec![] // No cache busting by default
    })),
    
    // Global cache duration limit
    max_ttl: Some(Duration::from_hours(24)),
    
    // Enable cache status headers for debugging
    cache_status_headers: true,
    
    ..Default::default()
};

let cache = HttpCache {
    mode: CacheMode::Default, // Can be overridden by cache_mode_fn and response_cache_mode_fn
    manager,
    options,
};
```

### Key Capabilities Summary

1. **Custom Cache Keys**: The `cache_key` function runs for every request, allowing complete customization of cache keys based on any request property
2. **Request-Based Cache Mode Override**: The `cache_mode_fn` allows overriding cache behavior based on request properties (headers, path, method, etc.)
3. **Response-Based Cache Mode Override**: The `response_cache_mode_fn` allows overriding cache behavior based on both request and response data
4. **Cache Busting**: The `cache_bust` function allows invalidating related cache entries
5. **Global Settings**: Options like `max_ttl` and `cache_status_headers` provide global configuration

All of these functions are called on a per-request basis, giving you complete control over caching behavior for each individual request.
