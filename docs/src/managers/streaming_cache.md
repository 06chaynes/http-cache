# StreamingManager (Streaming Cache)

[`StreamingManager`](https://github.com/06chaynes/http-cache/blob/main/http-cache/src/managers/streaming_cache.rs) is a file-based streaming cache manager that does not buffer response bodies in memory. This implementation stores response metadata and body content separately, enabling memory-efficient handling of large responses.

## Getting Started

The `StreamingManager` is built into the core `http-cache` crate and is available when the `streaming` feature is enabled.

```toml
[dependencies]
http-cache = { version = "1.0", features = ["streaming", "streaming-tokio"] }
```

Or for smol runtime:

```toml
[dependencies]  
http-cache = { version = "1.0", features = ["streaming", "streaming-smol"] }
```

## Basic Usage

```rust
use http_cache::{StreamingManager, StreamingBody, HttpStreamingCache};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a file-based streaming cache manager
    let cache_dir = PathBuf::from("./streaming-cache");
    let manager = StreamingManager::new(cache_dir);
    
    // Use with streaming cache
    let cache = HttpStreamingCache::new(manager);
    
    Ok(())
}
```

## Usage with Tower

The streaming cache manager works with Tower's `HttpCacheStreamingLayer`:

```rust
use http_cache::{StreamingManager, HttpCacheStreamingLayer};
use tower::{Service, ServiceExt};
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create streaming cache manager
    let cache_dir = PathBuf::from("./cache");
    let manager = StreamingManager::new(cache_dir);
    
    // Create streaming cache layer
    let cache_layer = HttpCacheStreamingLayer::new(manager);
    
    // Your base service
    let service = tower::service_fn(|_req: Request<Full<Bytes>>| async {
        Ok::<_, std::convert::Infallible>(
            Response::builder()
                .status(StatusCode::OK)
                .header("cache-control", "max-age=3600")
                .body(Full::new(Bytes::from("Large response data...")))?
        )
    });
    
    // Wrap with caching
    let cached_service = cache_layer.layer(service);
    
    // Make requests
    let request = Request::builder()
        .uri("https://example.com/large-file")
        .body(Full::new(Bytes::new()))?;
        
    let response = cached_service.oneshot(request).await?;
    println!("Response status: {}", response.status());
    
    Ok(())
}
```

## Working with the manager directly

### Creating a manager

```rust
use http_cache::StreamingManager;
use std::path::PathBuf;

// Create with custom cache directory
let cache_dir = PathBuf::from("./my-streaming-cache");
let manager = StreamingManager::new(cache_dir);
```

### Streaming Cache Operations

#### Caching a streaming response

```rust
use http_cache::StreamingManager;
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use http_cache_semantics::CachePolicy;
use url::Url;

let manager = StreamingManager::new(PathBuf::from("./cache"));

// Create a large response to cache
let large_data = vec![b'X'; 10_000_000]; // 10MB response
let response = Response::builder()
    .status(StatusCode::OK)
    .header("cache-control", "max-age=3600, public")
    .header("content-type", "application/octet-stream")
    .body(Full::new(Bytes::from(large_data)))?;

// Create cache policy
let request = Request::builder()
    .method("GET")
    .uri("https://example.com/large-file")
    .body(())?;
let policy = CachePolicy::new(&request, &Response::builder()
    .status(200)
    .header("cache-control", "max-age=3600, public")
    .body(vec![])?);

// Cache the response (content stored to disk, metadata separate)
let url = Url::parse("https://example.com/large-file")?;
let cached_response = manager.put(
    "GET:https://example.com/large-file".to_string(),
    response,
    policy,
    url,
).await?;

println!("Cached response without loading into memory!");
```

#### Retrieving a streaming response

```rust
// Retrieve from cache - returns a streaming body
let cached = manager.get("GET:https://example.com/large-file").await?;

if let Some((response, policy)) = cached {
    println!("Cache hit! Status: {}", response.status());
    
    // The response body streams directly from disk
    let body = response.into_body();
    
    // Process the streaming body without loading it all into memory
    let mut body_stream = std::pin::pin!(body);
    while let Some(frame_result) = body_stream.frame().await {
        let frame = frame_result?;
        if let Some(chunk) = frame.data_ref() {
            // Process chunk without accumulating in memory
            println!("Processing chunk of {} bytes", chunk.len());
        }
    }
} else {
    println!("Cache miss");
}
```

#### Deleting cached entries

```rust
// Remove from cache (deletes both metadata and content files)
manager.delete("GET:https://example.com/large-file").await?;
```

## Storage Structure

The StreamingManager organizes cache files as follows:

```text
cache-directory/
├── cache-v2/
│   ├── metadata/
│   │   ├── 1a2b3c4d....json  # Response metadata (headers, status, policy)
│   │   └── 5e6f7g8h....json
│   └── content/
│       ├── sha256_hash1      # Raw response body content
│       └── sha256_hash2
```

- **Metadata files**: JSON files containing response status, headers, cache policy, and content digest
- **Content files**: Raw binary content files identified by SHA256 hash for deduplication
- **Content-addressable**: Identical content is stored only once regardless of URL

## Performance Characteristics

### Memory Usage

- **Constant memory usage** regardless of response size
- Only metadata loaded into memory (~few KB per response)
- Response bodies stream directly from disk files

### Disk Usage

- **Content deduplication** via SHA256 hashing
- **Efficient storage** with separate metadata and content
- **Persistent cache** survives application restarts

### Use Cases

- **Large file responses** (images, videos, archives)
- **Memory-constrained environments**
- **High-throughput applications** with large responses
- **Long-running services** that need persistent caching

## Comparison with Other Managers

| Manager | Memory Usage | Storage | Streaming | Best For |
|---------|--------------|---------|-----------|----------|
| StreamingManager | Constant | Disk | Yes | Large responses, memory efficiency |
| CACacheManager | Buffers responses | Disk | No | General purpose, moderate sizes |
| MokaManager | Buffers responses | Memory | No | Fast access, small responses |
| QuickManager | Buffers responses | Memory | No | Low overhead, small responses |

## Configuration

The StreamingManager uses sensible defaults but can be configured through environment:

```rust
// Cache directory structure is automatically created
let manager = StreamingManager::new(PathBuf::from("./cache"));

// The manager handles:
// - Directory creation
// - Content deduplication  
// - Metadata organization
// - File cleanup on delete
```

For advanced configuration, you can implement custom cleanup policies or directory management by extending the manager.
