# Rate Limiting

The http-cache library provides built-in cache-aware rate limiting functionality that only applies when making actual network requests (cache misses), not when serving responses from cache (cache hits).

This feature is available behind the `rate-limiting` feature flag and provides an elegant solution for scraping scenarios where you want to cache responses to avoid rate limits, but still need to respect rate limits for new requests.

## How It Works

The rate limiting follows this flow:

1. **Check cache first** - The cache is checked for an existing response
2. **If cache hit** - Return the cached response immediately (no rate limiting applied)
3. **If cache miss** - Apply rate limiting before making the network request
4. **Make network request** - Fetch from the remote server after rate limiting
5. **Cache and return** - Store the response and return it

This ensures that:
- Cached responses are served instantly without any rate limiting delays
- Only actual network requests are rate limited
- Multiple cache hits can be served concurrently without waiting

## Rate Limiting Strategies

### DomainRateLimiter

Applies rate limiting per domain, allowing different rate limits for different hosts:

```rust
use http_cache::rate_limiting::{DomainRateLimiter, Quota};
use std::num::NonZeroU32;
use std::sync::Arc;

// Allow 10 requests per second per domain
let quota = Quota::per_second(NonZeroU32::new(10).unwrap());
let rate_limiter = Arc::new(DomainRateLimiter::new(quota));
```

### DirectRateLimiter

Applies a global rate limit across all requests regardless of domain:

```rust
use http_cache::rate_limiting::{DirectRateLimiter, Quota};
use std::num::NonZeroU32;
use std::sync::Arc;

// Allow 5 requests per second globally
let quota = Quota::per_second(NonZeroU32::new(5).unwrap());
let rate_limiter = Arc::new(DirectRateLimiter::direct(quota));
```

### Custom Rate Limiters

You can implement your own rate limiting strategy by implementing the `CacheAwareRateLimiter` trait:

```rust
use http_cache::rate_limiting::CacheAwareRateLimiter;
use async_trait::async_trait;

struct CustomRateLimiter {
    // Your custom rate limiting logic
}

#[async_trait]
impl CacheAwareRateLimiter for CustomRateLimiter {
    async fn until_key_ready(&self, key: &str) {
        // Implement your rate limiting logic here
        // This method should block until it's safe to make a request
    }

    fn check_key(&self, key: &str) -> bool {
        // Return true if a request can be made immediately
        // Return false if rate limiting would apply
        true
    }
}
```

## Configuration

Rate limiting is configured through the `HttpCacheOptions` struct:

```rust
use http_cache::{HttpCache, HttpCacheOptions, CacheMode};
use http_cache::rate_limiting::{DomainRateLimiter, Quota};
use std::sync::Arc;

let quota = Quota::per_second(std::num::NonZeroU32::new(10).unwrap());
let rate_limiter = Arc::new(DomainRateLimiter::new(quota));

let cache = HttpCache {
    mode: CacheMode::Default,
    manager: your_cache_manager,
    options: HttpCacheOptions {
        rate_limiter: Some(rate_limiter),
        ..Default::default()
    },
};
```

## Client-Specific Examples

### reqwest

```rust
use http_cache_reqwest::{Cache, HttpCache, CACacheManager, CacheMode, HttpCacheOptions};
use http_cache_reqwest::{DomainRateLimiter, Quota};
use reqwest_middleware::ClientBuilder;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let quota = Quota::per_second(std::num::NonZeroU32::new(5).unwrap());
    let rate_limiter = Arc::new(DomainRateLimiter::new(quota));
    
    let client = ClientBuilder::new(reqwest::Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager::new("./cache".into(), true),
            options: HttpCacheOptions {
                rate_limiter: Some(rate_limiter),
                ..Default::default()
            },
        }))
        .build();

    // First request - will be rate limited and cached
    let resp1 = client.get("https://httpbin.org/delay/1").send().await?;
    println!("First response: {}", resp1.status());

    // Second identical request - served from cache, no rate limiting
    let resp2 = client.get("https://httpbin.org/delay/1").send().await?;
    println!("Second response: {}", resp2.status());

    Ok(())
}
```

### surf

```rust
use http_cache_surf::{Cache, HttpCache, CACacheManager, CacheMode, HttpCacheOptions};
use http_cache_surf::{DomainRateLimiter, Quota};
use surf::Client;
use std::sync::Arc;
use macro_rules_attribute::apply;
use smol_macros::main;

#[apply(main!)]
async fn main() -> surf::Result<()> {
    let quota = Quota::per_second(std::num::NonZeroU32::new(5).unwrap());
    let rate_limiter = Arc::new(DomainRateLimiter::new(quota));
    
    let client = Client::new()
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager::new("./cache".into(), true),
            options: HttpCacheOptions {
                rate_limiter: Some(rate_limiter),
                ..Default::default()
            },
        }));

    // Requests will be rate limited on cache misses only
    let mut resp1 = client.get("https://httpbin.org/delay/1").await?;
    println!("First response: {}", resp1.body_string().await?);

    let mut resp2 = client.get("https://httpbin.org/delay/1").await?;
    println!("Second response: {}", resp2.body_string().await?);

    Ok(())
}
```

### tower

```rust
use http_cache_tower::{HttpCacheLayer, CACacheManager};
use http_cache::{CacheMode, HttpCache, HttpCacheOptions};
use http_cache_tower::{DomainRateLimiter, Quota};
use tower::ServiceBuilder;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let quota = Quota::per_second(std::num::NonZeroU32::new(5).unwrap());
    let rate_limiter = Arc::new(DomainRateLimiter::new(quota));
    
    let cache = HttpCache {
        mode: CacheMode::Default,
        manager: CACacheManager::new("./cache".into(), true),
        options: HttpCacheOptions {
            rate_limiter: Some(rate_limiter),
            ..Default::default()
        },
    };

    let service = ServiceBuilder::new()
        .layer(HttpCacheLayer::with_cache(cache))
        .service_fn(your_service_function);

    // Use the service - rate limiting will be applied on cache misses
}
```

### ureq

```rust
use http_cache_ureq::{CachedAgent, CACacheManager, CacheMode, HttpCacheOptions};
use http_cache_ureq::{DomainRateLimiter, Quota};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    smol::block_on(async {
        let quota = Quota::per_second(std::num::NonZeroU32::new(5).unwrap());
        let rate_limiter = Arc::new(DomainRateLimiter::new(quota));
        
        let agent = CachedAgent::builder()
            .cache_manager(CACacheManager::new("./cache".into(), true))
            .cache_mode(CacheMode::Default)
            .cache_options(HttpCacheOptions {
                rate_limiter: Some(rate_limiter),
                ..Default::default()
            })
            .build()?;

        // Rate limiting applies only on cache misses
        let response1 = agent.get("https://httpbin.org/delay/1").call().await?;
        println!("First response: {}", response1.status());

        let response2 = agent.get("https://httpbin.org/delay/1").call().await?;
        println!("Second response: {}", response2.status());

        Ok(())
    })
}
```

## Use Cases

This cache-aware rate limiting is particularly useful for:

- **Web scraping** - Cache responses to avoid repeated requests while respecting rate limits for new content
- **API clients** - Improve performance with caching while staying within API rate limits
- **Data collection** - Efficiently gather data without overwhelming servers
- **Development and testing** - Reduce API calls during development while maintaining realistic rate limiting behavior

## Streaming Support

Rate limiting works seamlessly with streaming cache operations. When using streaming managers or streaming middleware, rate limiting is applied in the same cache-aware manner:

### Streaming Cache Examples

#### reqwest Streaming with Rate Limiting

```rust
use http_cache_reqwest::{StreamingCache, HttpCacheOptions};
use http_cache::{StreamingManager, CacheMode};
use http_cache_reqwest::{DomainRateLimiter, Quota};
use reqwest_middleware::ClientBuilder;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let quota = Quota::per_second(std::num::NonZeroU32::new(2).unwrap());
    let rate_limiter = Arc::new(DomainRateLimiter::new(quota));
    
    let streaming_manager = StreamingManager::new("./streaming-cache".into());
    
    let client = ClientBuilder::new(reqwest::Client::new())
        .with(StreamingCache::with_options(
            streaming_manager, 
            CacheMode::Default,
            HttpCacheOptions {
                rate_limiter: Some(rate_limiter),
                ..Default::default()
            }
        ))
        .build();

    // First request - rate limited and cached as streaming
    let resp1 = client.get("https://httpbin.org/stream-bytes/10000").send().await?;
    println!("First streaming response: {}", resp1.status());

    // Second request - served from streaming cache, no rate limiting
    let resp2 = client.get("https://httpbin.org/stream-bytes/10000").send().await?;
    println!("Second streaming response: {}", resp2.status());

    Ok(())
}
```

#### tower Streaming with Rate Limiting

```rust
use http_cache_tower::{HttpCacheStreamingLayer};
use http_cache::{StreamingManager, CacheMode, HttpCacheOptions};
use http_cache_tower::{DomainRateLimiter, Quota};
use tower::ServiceBuilder;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let quota = Quota::per_second(std::num::NonZeroU32::new(3).unwrap());
    let rate_limiter = Arc::new(DomainRateLimiter::new(quota));
    
    let streaming_manager = StreamingManager::new("./streaming-cache".into());
    
    let layer = HttpCacheStreamingLayer::with_options(
        streaming_manager,
        HttpCacheOptions {
            rate_limiter: Some(rate_limiter),
            ..Default::default()
        }
    );

    let service = ServiceBuilder::new()
        .layer(layer)
        .service_fn(your_streaming_service_function);

    // Streaming responses will be rate limited on cache misses only
}
```

### Streaming Rate Limiting Benefits

When using streaming with rate limiting:

- **Memory efficiency** - Large responses are streamed without full buffering
- **Cache-aware rate limiting** - Rate limits only apply to actual network requests, not streaming from cache
- **Concurrent streaming** - Multiple cached streams can be served simultaneously without rate limiting delays
- **Efficient large file handling** - Perfect for scenarios involving large files or media content

## Performance Benefits

By only applying rate limiting on cache misses, you get:

- **Instant cache hits** - No rate limiting delays for cached responses
- **Concurrent cache serving** - Multiple cache hits can be served simultaneously
- **Efficient scraping** - Re-scraping cached content doesn't count against rate limits
- **Better user experience** - Faster response times for frequently accessed resources
- **Streaming optimization** - Large cached responses stream immediately without rate limiting overhead