# ureq

The [`http-cache-ureq`](https://github.com/06chaynes/http-cache/tree/main/http-cache-ureq) crate provides HTTP caching for the [`ureq`](https://github.com/algesten/ureq) HTTP client.

Since ureq is a synchronous HTTP client, this implementation uses the [smol](https://github.com/smol-rs/smol) async runtime to integrate with the async http-cache system. The caching wrapper preserves ureq's synchronous interface while providing async caching capabilities internally.

## Features

- `json` - Enables JSON request/response support via `send_json()` and `into_json()` methods (requires `serde_json`)
- `manager-cacache` - Enable [cacache](https://docs.rs/cacache/) cache manager (default)
- `manager-moka` - Enable [moka](https://docs.rs/moka/) cache manager

## Basic Usage

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
http-cache-ureq = "1.0.0-alpha.1"
```

Use the `CachedAgent` builder to create a cached HTTP client:

```rust
use http_cache_ureq::{CachedAgent, CACacheManager, CacheMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    smol::block_on(async {
        let agent = CachedAgent::builder()
            .cache_manager(CACacheManager::new("./cache".into(), true))
            .cache_mode(CacheMode::Default)
            .build()?;
        
        // This request will be cached according to response headers
        let response = agent.get("https://httpbin.org/cache/60").call().await?;
        println!("Status: {}", response.status());
        println!("Cached: {}", response.is_cached());
        println!("Response: {}", response.into_string()?);
        
        // Subsequent identical requests may be served from cache
        let cached_response = agent.get("https://httpbin.org/cache/60").call().await?;
        println!("Cached status: {}", cached_response.status());
        println!("Is cached: {}", cached_response.is_cached());
        println!("Cached response: {}", cached_response.into_string()?);
        
        Ok(())
    })
}
```

## JSON Support

Enable the `json` feature to send and parse JSON data:

```toml
[dependencies]
http-cache-ureq = { version = "1.0.0-alpha.1", features = ["json"] }
```

```rust
use http_cache_ureq::{CachedAgent, CACacheManager, CacheMode};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    smol::block_on(async {
        let agent = CachedAgent::builder()
            .cache_manager(CACacheManager::new("./cache".into(), true))
            .cache_mode(CacheMode::Default)
            .build()?;
        
        // Send JSON data
        let response = agent.post("https://httpbin.org/post")
            .send_json(json!({"key": "value"}))
            .await?;
        
        // Parse JSON response
        let json: serde_json::Value = response.into_json()?;
        println!("Response: {}", json);
        
        Ok(())
    })
}
```

## Cache Modes

Control caching behavior with different modes:

```rust
use http_cache_ureq::{CachedAgent, CACacheManager, CacheMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    smol::block_on(async {
        let agent = CachedAgent::builder()
            .cache_manager(CACacheManager::new("./cache".into(), true))
            .cache_mode(CacheMode::ForceCache) // Cache everything, ignore headers
            .build()?;
        
        // This will be cached even if headers say not to cache
        let response = agent.get("https://httpbin.org/uuid").call().await?;
        println!("Response: {}", response.into_string()?);
        
        Ok(())
    })
}
```

## Custom ureq Configuration

Preserve your ureq agent configuration while adding caching:

```rust
use http_cache_ureq::{CachedAgent, CACacheManager, CacheMode};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    smol::block_on(async {
        // Create custom ureq configuration
        let config = ureq::config::Config::builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent("MyApp/1.0")
            .build();
        
        let agent = CachedAgent::builder()
            .agent_config(config)
            .cache_manager(CACacheManager::new("./cache".into(), true))
            .cache_mode(CacheMode::Default)
            .build()?;
        
        let response = agent.get("https://httpbin.org/cache/60").call().await?;
        println!("Response: {}", response.into_string()?);
        
        Ok(())
    })
}
```

## In-Memory Caching

Use the Moka in-memory cache:

```toml
[dependencies]
http-cache-ureq = { version = "1.0.0-alpha.1", features = ["manager-moka"] }
```

```rust
use http_cache_ureq::{CachedAgent, MokaManager, MokaCache, CacheMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    smol::block_on(async {
        let agent = CachedAgent::builder()
            .cache_manager(MokaManager::new(MokaCache::new(1000))) // Max 1000 entries
            .cache_mode(CacheMode::Default)
            .build()?;
            
        let response = agent.get("https://httpbin.org/cache/60").call().await?;
        println!("Response: {}", response.into_string()?);
        
        Ok(())
    })
}
```

## Maximum TTL Control

Control cache expiration times, particularly useful with `IgnoreRules` mode:

```rust
use http_cache_ureq::{CachedAgent, CACacheManager, CacheMode, HttpCacheOptions};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    smol::block_on(async {
        let agent = CachedAgent::builder()
            .cache_manager(CACacheManager::new("./cache".into(), true))
            .cache_mode(CacheMode::IgnoreRules) // Ignore server cache headers
            .cache_options(HttpCacheOptions {
                max_ttl: Some(Duration::from_secs(300)), // Limit cache to 5 minutes maximum
                ..Default::default()
            })
            .build()?;
        
        // This will be cached for max 5 minutes even if server says cache longer
        let response = agent.get("https://httpbin.org/cache/3600").call().await?;
        println!("Response: {}", response.into_string()?);
        
        Ok(())
    })
}
```

## Implementation Notes

- The wrapper preserves ureq's synchronous interface while using async caching internally
- The `http_status_as_error` setting is automatically disabled to ensure proper cache operation
- All HTTP methods are supported (GET, POST, PUT, DELETE, HEAD, etc.)
- Cache invalidation occurs for non-GET/HEAD requests to the same resource
- Only GET and HEAD requests are cached by default
- `max_ttl` provides expiration control when using `CacheMode::IgnoreRules`