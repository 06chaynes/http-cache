# Server-Side Caching

Server-side HTTP response caching is fundamentally different from client-side caching. While client-side middleware caches responses from external APIs, server-side middleware caches your own application's responses to reduce load and improve performance.

## What is Server-Side Caching?

Server-side caching stores the responses your application generates so that subsequent identical requests can be served from cache without re-executing expensive operations like database queries or complex computations.

### Example Flow

**Without Server-Side Caching:**
```
Request → Routing → Handler → Database Query → Response (200ms)
Request → Routing → Handler → Database Query → Response (200ms)
Request → Routing → Handler → Database Query → Response (200ms)
```

**With Server-Side Caching:**
```
Request → Routing → Cache MISS → Handler → Database Query → Response (200ms) → Cached
Request → Routing → Cache HIT → Response (2ms)
Request → Routing → Cache HIT → Response (2ms)
```

## Key Differences from Client-Side Caching

| Aspect | Client-Side | Server-Side |
|--------|-------------|-------------|
| **What it caches** | External API responses | Your app's responses |
| **Position** | Before making outbound requests | After routing, before handlers |
| **Use case** | Reduce external API calls | Reduce internal computation |
| **RFC 7234 behavior** | Client cache rules | Shared cache rules |
| **Request extensions** | N/A | Must preserve (path params, state) |

## Available Implementations

Currently, server-side caching is available for:

- **Tower-based servers** (Axum, Hyper, Tonic) - See [tower-server](./tower-server.md)

## When to Use Server-Side Caching

### Good Use Cases ✅

1. **Public API endpoints** with expensive database queries
2. **Read-heavy workloads** where data doesn't change frequently
3. **Dashboard or analytics data** that updates periodically
4. **Static-like content** that requires dynamic generation
5. **Search results** for common queries
6. **Rendered HTML** for public pages

### Avoid Caching ❌

1. **User-specific data** (unless using proper cache key differentiation)
2. **Authenticated endpoints** (without user ID in cache key)
3. **Real-time data** that must always be fresh
4. **Write operations** (POST/PUT/DELETE requests)
5. **Sensitive information** that shouldn't be shared
6. **Session-dependent responses** (without session ID in cache key)

## Security Considerations

Server-side caches are **shared caches** - cached responses are served to ALL users. This is different from client-side caches which are per-client.

### Critical Security Rule

**Never cache user-specific data without including the user/session identifier in the cache key.**

### Safe Patterns

**Pattern 1: Mark user-specific responses as private**
```rust
async fn user_profile() -> Response {
    (
        [(header::CACHE_CONTROL, "private")], // Won't be cached
        "User profile data"
    ).into_response()
}
```

**Pattern 2: Include user ID in cache key**
```rust
let keyer = CustomKeyer::new(|req: &Request<()>| {
    let user_id = extract_user_id(req);
    format!("{} {} user:{}", req.method(), req.uri().path(), user_id)
});
```

**Pattern 3: Don't cache at all**
```rust
async fn sensitive_data() -> Response {
    (
        [(header::CACHE_CONTROL, "no-store")],
        "Sensitive data"
    ).into_response()
}
```

## RFC 7234 Compliance

Server-side caches implement **shared cache** semantics as defined in RFC 7234:

### Must NOT Cache

- Responses with `Cache-Control: private` (user-specific)
- Responses with `Cache-Control: no-store` (sensitive)
- Responses with `Cache-Control: no-cache` (requires revalidation)
- Non-2xx status codes (errors)
- Requests with `Authorization` header (unless response explicitly allows via `public`, `s-maxage`, or `must-revalidate`)

### Must Cache Correctly

- Prefer `s-maxage` over `max-age` (shared cache specific)
- Respect `Vary` headers (content negotiation)
- Handle `Expires` header as fallback
- Support `max-age` and `public` directives

## Performance Characteristics

### Benefits

- **Reduced database load**: Cached responses don't hit the database
- **Lower CPU usage**: Expensive computations run once
- **Faster response times**: Cache hits are typically <5ms
- **Better scalability**: Handle more requests with same resources

### Considerations

- **Memory usage**: Cached responses stored in memory or disk
- **Stale data**: Cached data may become outdated
- **Cache warming**: Initial requests (cache misses) are slower
- **Invalidation complexity**: Updating cached data can be tricky

## Cache Invalidation Strategies

### Time-Based (TTL)

Set expiration times on cached responses:

```rust
async fn handler() -> Response {
    (
        [(header::CACHE_CONTROL, "max-age=300")], // 5 minutes
        "Response data"
    ).into_response()
}
```

### Event-Based

Manually invalidate cache entries when data changes:

```rust
// After updating user data
cache_manager.delete(&format!("GET /users/{}", user_id)).await?;
```

### Hybrid Approach

Combine TTL with manual invalidation:
- Use TTL for automatic expiration
- Invalidate early when you know data changed

## Best Practices

1. **Start conservative**: Use shorter TTLs initially, increase as you gain confidence
2. **Monitor cache hit rates**: Track X-Cache headers to measure effectiveness
3. **Set size limits**: Prevent cache from consuming too much memory
4. **Use appropriate keyers**: Match cache key strategy to your needs
5. **Document caching behavior**: Make it clear which endpoints are cached
6. **Test cache invalidation**: Ensure updates propagate correctly
7. **Consider cache warming**: Pre-populate cache for common requests
8. **Handle cache failures gracefully**: Application should work even if cache fails

## Monitoring and Debugging

### Enable Cache Status Headers

```rust
let options = ServerCacheOptions {
    cache_status_headers: true,
    ..Default::default()
};
```

This adds `X-Cache` headers to responses:
- `X-Cache: HIT` - Served from cache
- `X-Cache: MISS` - Generated by handler

### Track Metrics

Monitor these key metrics:
- Cache hit rate (hits / total requests)
- Average response time (hits vs misses)
- Cache size and memory usage
- Cache eviction rate
- Stale response rate

## Getting Started

See the [tower-server](./tower-server.md) documentation for detailed implementation guide.
