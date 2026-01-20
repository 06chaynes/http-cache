# foyer

[`foyer`](https://github.com/foyer-rs/foyer) is a hybrid in-memory + disk cache that provides configurable eviction strategies, optional disk storage, request deduplication, and Tokio-native async operations. The `http-cache` implementation provides traditional buffered caching capabilities using the `FoyerManager`.

## Getting Started

The `foyer` backend cache manager is available when the `manager-foyer` feature is enabled.

```toml
[dependencies]
http-cache = { version = "1.0", features = ["manager-foyer"] }
```

## Basic Usage

### In-Memory Only Cache

For a simple memory-only cache:

```rust
use http_cache::FoyerManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an in-memory cache with capacity for 1000 entries
    let manager = FoyerManager::in_memory(1000).await?;

    Ok(())
}
```

### Hybrid Cache (Memory + Disk)

For a hybrid cache with both memory and disk storage:

```rust
use http_cache::FoyerManager;
use foyer::{HybridCacheBuilder, Engine};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build a hybrid cache with memory and disk storage
    let cache = HybridCacheBuilder::new()
        .memory(64)  // Memory capacity in entries
        .storage(Engine::Large)
        .with_device_options(
            foyer::DirectFsDeviceOptionsBuilder::new(PathBuf::from("./cache"))
                .with_capacity(256 * 1024 * 1024)  // 256 MB disk capacity
                .build()
        )
        .build()
        .await?;

    let manager = FoyerManager::new(cache);

    Ok(())
}
```

## Features

- **Hybrid Storage**: Combine fast in-memory caching with persistent disk storage
- **Configurable Eviction**: Support for w-TinyLFU, S3-FIFO, and SIEVE eviction strategies
- **Request Deduplication**: Automatic deduplication of concurrent requests for the same key
- **Tokio-Native**: Built for async Rust with native Tokio support

## Working with the Manager Directly

### Creating a Manager

```rust
use http_cache::FoyerManager;
use foyer::HybridCacheBuilder;

// In-memory only (simple)
let manager = FoyerManager::in_memory(1000).await?;

// Or with custom configuration
let cache = HybridCacheBuilder::new()
    .memory(100)
    .storage(foyer::Engine::Large)
    .build()
    .await?;
let manager = FoyerManager::new(cache);
```

### Cache Operations

The `FoyerManager` implements the `CacheManager` trait:

```rust
use http_cache::{CacheManager, HttpResponse, HttpVersion};
use http_cache_semantics::CachePolicy;
use url::Url;

// Retrieve a cached response
let response = manager.get("my-cache-key").await?;

// Store a response in the cache
let url = Url::parse("http://example.com")?;
let response = HttpResponse {
    body: b"response body".to_vec(),
    headers: Default::default(),
    status: 200,
    url: url.clone(),
    version: HttpVersion::Http11,
};
let req = http::Request::get("http://example.com").body(())?;
let res = http::Response::builder()
    .status(200)
    .body(b"response body".to_vec())?;
let policy = CachePolicy::new(&req, &res);
let cached = manager.put("my-cache-key".into(), response, policy).await?;

// Remove from cache
manager.delete("my-cache-key").await?;
```

### Graceful Shutdown

When using disk storage, call `close()` before application exit to ensure data is flushed:

```rust
// Before application exit
manager.close().await?;
```

## When to Use Foyer

`FoyerManager` is ideal for:

- **Large Caches**: When you need both in-memory speed and disk persistence
- **Configurable Eviction**: When you need fine-grained control over eviction policies
- **High Concurrency**: When you have many concurrent requests for the same resources
- **Persistence**: When cached data should survive application restarts

For simpler use cases, consider:
- `CACacheManager` for disk-only caching
- `MokaManager` for memory-only caching with simple configuration
- `QuickManager` for lightweight in-memory caching
