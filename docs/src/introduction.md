# Introduction

`http-cache` is a comprehensive library for HTTP response caching in Rust. It provides both **client-side** and **server-side** caching middleware for multiple HTTP clients and frameworks. Built on top of [`http-cache-semantics`](https://github.com/kornelski/rusty-http-cache-semantics), it correctly implements HTTP cache semantics as defined in RFC 7234.

## Key Features

- **Client-Side Caching**: Cache responses from external APIs you're calling
- **Server-Side Caching**: Cache your own application's responses to reduce load
- **Traditional Caching**: Standard HTTP response caching with full buffering
- **Streaming Support**: Memory-efficient caching for large responses without full buffering
- **Cache-Aware Rate Limiting**: Intelligent rate limiting that only applies on cache misses
- **Multiple Backends**: Support for disk-based (cacache) and in-memory (moka, quick-cache) storage
- **Client Integrations**: Support for reqwest, surf, tower, and ureq HTTP clients
- **Server Framework Support**: Tower-based servers (Axum, Hyper, Tonic)
- **RFC 7234 Compliance**: Proper HTTP cache semantics with respect for cache-control headers

## Client-Side vs Server-Side Caching

### Client-Side Caching

Cache responses from external APIs your application calls:

```rust
// Example: Caching API responses you fetch
let client = reqwest::Client::new();
let cached_client = HttpCache::new(client, cache_manager);
let response = cached_client.get("https://api.example.com/users").send().await?;
```

**Use cases:**
- Reducing calls to external APIs
- Offline support
- Bandwidth optimization
- Rate limit compliance

### Server-Side Caching

Cache responses your application generates:

```rust
// Example: Caching your own endpoint responses
let app = Router::new()
    .route("/users/:id", get(get_user))
    .layer(ServerCacheLayer::new(cache_manager)); // Cache your responses
```

**Use cases:**
- Reducing database queries
- Caching expensive computations
- Improving response times
- Reducing server load

**Critical:** Server-side cache middleware must be placed **after** routing to preserve request context (path parameters, state, etc.).

## Streaming vs Traditional Caching

The library supports two caching approaches:

- **Traditional Caching** (`CacheManager` trait): Buffers entire responses in memory before caching. Suitable for smaller responses and simpler use cases.
- **Streaming Caching** (`StreamingCacheManager` trait): Processes responses as streams without full buffering. Ideal for large files, media content, or memory-constrained environments.

Note: Streaming is currently only available for client-side caching. Server-side caching uses buffered responses.
