# StreamingManager (Streaming Cache)

[`StreamingManager`](https://github.com/06chaynes/http-cache/blob/main/http-cache/src/managers/streaming_cache.rs) is a streaming cache manager that combines [cacache](https://github.com/zkat/cacache-rs) for disk storage with [moka](https://github.com/moka-rs/moka) for metadata tracking and TinyLFU eviction.

## Key Features

- **True streaming on read**: Cached responses are streamed from disk in 64KB chunks, not loaded fully into memory
- **TinyLFU eviction**: Better hit rates than simple LRU by filtering out one-hit wonders
- **Content deduplication**: Automatic via cacache's content-addressed storage
- **Integrity verification**: Cached data is verified on read
- **Body size limits**: Configurable maximum body size to prevent memory exhaustion
- **Backpressure handling**: Eviction cleanup uses bounded channels to prevent task accumulation

## Important: Write-Path Buffering

While cached responses are streamed on **read** (GET), the **write** path (PUT) requires buffering the entire response body in memory. This is necessary to:
- Compute the content hash for cacache's content-addressed storage
- Enable content deduplication

For very large responses, configure the `max_body_size` limit to prevent OOM. Memory usage during PUT is O(response_size), not O(buffer_size). The default limit is 100MB.

## Getting Started

The `StreamingManager` is built into the core `http-cache` crate and is available when the `streaming` feature is enabled.

```toml
[dependencies]
http-cache = { version = "1.0", features = ["streaming"] }
```

## Basic Usage

```rust
use http_cache::StreamingManager;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a streaming cache manager with disk storage
    let manager = StreamingManager::new(
        PathBuf::from("./cache"),  // Cache directory
        10_000,                     // Max entries
    ).await?;

    // Or use with_temp_dir() for testing (uses temp directory)
    let test_manager = StreamingManager::with_temp_dir(1000).await?;

    // For custom body size limits (e.g., 50MB max)
    let custom_manager = StreamingManager::with_max_body_size(
        PathBuf::from("./cache"),
        10_000,
        50 * 1024 * 1024,
    ).await?;

    Ok(())
}
```

## Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│  moka::Cache<String, CacheMetadata>  (in-memory)               │
│  - Tracks: key → {content_hash, policy, headers, size}         │
│  - TinyLFU eviction (better hit rates than LRU)                │
│  - Eviction listener sends to bounded cleanup channel          │
└─────────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  cacache (disk)                                                 │
│  - Content-addressed storage (automatic deduplication)         │
│  - Streaming reads via AsyncRead (64KB chunks)                 │
│  - Integrity verification built-in                              │
└─────────────────────────────────────────────────────────────────┘
```

## Memory Efficiency

**On cache hit (GET):** Only ~64KB is held in memory at a time (the streaming buffer), regardless of response size:

| Response Size | Peak Memory (Buffered) | Peak Memory (Streaming GET) |
|---------------|------------------------|-----------------------------|
| 100KB | 100KB | ~64KB |
| 1MB | 1MB | ~64KB |
| 10MB | 10MB | ~64KB |
| 100MB | 100MB | ~64KB |

**On cache write (PUT):** The entire response body is buffered in memory to compute the content hash. This is limited by `max_body_size` (default: 100MB) to prevent memory exhaustion.

## Usage with Tower

```rust
use http_cache::StreamingManager;
use http_cache_tower::HttpCacheStreamingLayer;
use tower::{Service, ServiceExt};
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create streaming cache manager
    let manager = StreamingManager::new(
        PathBuf::from("./cache"),
        10_000,
    ).await?;

    // Create streaming cache layer
    let cache_layer = HttpCacheStreamingLayer::new(manager);

    // Your base service
    let service = tower::service_fn(|_req: Request<Full<Bytes>>| async {
        Ok::<_, std::convert::Infallible>(
            Response::builder()
                .status(StatusCode::OK)
                .header("cache-control", "max-age=3600")
                .body(Full::new(Bytes::from("Response data...")))?
        )
    });

    // Wrap with caching
    let cached_service = cache_layer.layer(service);

    // Make requests
    let request = Request::builder()
        .uri("https://example.com/api")
        .body(Full::new(Bytes::new()))?;

    let response = cached_service.oneshot(request).await?;
    println!("Response status: {}", response.status());

    Ok(())
}
```

## Usage with Reqwest

```rust
use http_cache::StreamingManager;
use http_cache_reqwest::{StreamingCache, CacheMode};
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = StreamingManager::new(
        PathBuf::from("./cache"),
        10_000,
    ).await?;

    let client = ClientBuilder::new(Client::new())
        .with(StreamingCache::new(manager, CacheMode::Default))
        .build();

    let response = client
        .get("https://httpbin.org/get")
        .send()
        .await?;

    println!("Status: {}", response.status());
    Ok(())
}
```

## Working with the manager directly

### Caching a response

```rust
use http_cache::{StreamingManager, StreamingCacheManager};
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use http_cache_semantics::CachePolicy;
use url::Url;
use std::path::PathBuf;

let manager = StreamingManager::new(PathBuf::from("./cache"), 10_000).await?;

// Create a response to cache
let response = Response::builder()
    .status(StatusCode::OK)
    .header("cache-control", "max-age=3600, public")
    .header("content-type", "application/json")
    .body(Full::new(Bytes::from(r#"{"data": "example"}"#)))?;

// Create cache policy
let request = Request::builder()
    .method("GET")
    .uri("https://example.com/api")
    .body(())?;
let policy = CachePolicy::new(&request, &Response::builder()
    .status(200)
    .header("cache-control", "max-age=3600, public")
    .body(vec![])?);

// Cache the response
let url = Url::parse("https://example.com/api")?;
let cached_response = manager.put(
    "GET:https://example.com/api".to_string(),
    response,
    policy,
    url,
    None, // optional metadata
).await?;
```

### Retrieving a cached response

```rust
// Retrieve from cache - body is streamed from disk!
let cached = manager.get("GET:https://example.com/api").await?;

if let Some((response, policy)) = cached {
    println!("Cache hit! Status: {}", response.status());

    // The response body streams from disk in 8KB chunks
    // Memory usage stays constant regardless of body size
    use http_body_util::BodyExt;
    let body = response.into_body();
    let bytes = body.collect().await?.to_bytes();
    println!("Body: {} bytes", bytes.len());
} else {
    println!("Cache miss");
}
```

### Deleting cached entries

```rust
// Remove from cache
manager.delete("GET:https://example.com/api").await?;
```

### Cache management

```rust
// Get the number of entries
let count = manager.entry_count();

// Clear all entries
manager.clear().await?;

// Run pending maintenance tasks
manager.run_pending_tasks().await;
```

## Content Deduplication

The cacache backend automatically deduplicates content. If two different URLs return the same response body, it's only stored once on disk:

```text
Request 1: GET /api/users → 1MB response (hash: abc123)
  └─ metadata/key1.json → points to content/abc123

Request 2: GET /api/users?v=2 → Same 1MB response (hash: abc123)
  └─ metadata/key2.json → points to content/abc123 (same file!)

Storage: 1MB (not 2MB)
```

## Comparison with Buffered Caching

| Aspect | CACacheManager (Buffered) | StreamingManager |
|--------|---------------------------|------------------|
| **Memory on GET** | Full body in memory | ~64KB streaming buffer |
| **Memory on PUT** | Full body in memory | Full body in memory (limited by max_body_size) |
| **Eviction** | Manual/None | TinyLFU (automatic) |
| **Content Dedup** | Yes (cacache) | Yes (cacache) |
| **Large responses** | May OOM | Configurable limit, streaming on read |
| **Body size limit** | None | Configurable (default 100MB) |
| **Use case** | Small responses | Large responses |
