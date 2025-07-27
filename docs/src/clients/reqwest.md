# reqwest

The [`http-cache-reqwest`](https://github.com/06chaynes/http-cache/tree/main/http-cache-reqwest) crate provides a [`Middleware`](https://docs.rs/http-cache/latest/http_cache/trait.Middleware.html) implementation for the [`reqwest`](https://github.com/seanmonstar/reqwest) HTTP client. It accomplishes this by utilizing [`reqwest_middleware`](https://github.com/TrueLayer/reqwest-middleware).

## Getting Started

```sh
cargo add http-cache-reqwest
```

## Features

- `manager-cacache`: (default) Enables the [`CACacheManager`](https://docs.rs/http-cache/latest/http_cache/struct.CACacheManager.html) backend cache manager.
- `manager-moka`: Enables the [`MokaManager`](https://docs.rs/http-cache/latest/http_cache/struct.MokaManager.html) backend cache manager.
- `streaming`: Enables streaming cache support for memory-efficient handling of large response bodies.

## Usage

In the following example we will construct our client using the builder provided by [`reqwest_middleware`](https://github.com/TrueLayer/reqwest-middleware) with our cache struct from [`http-cache-reqwest`](https://github.com/06chaynes/http-cache/tree/latest/http-cache-reqwest). This example will use the default mode, default cacache manager, and default http cache options.

After constructing our client, we will make a request to the [MDN Caching Docs](https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching) which should result in an object stored in cache on disk.

```rust
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, Result};
use http_cache_reqwest::{Cache, CacheMode, CACacheManager, HttpCache, HttpCacheOptions};

#[tokio::main]
async fn main() -> Result<()> {
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
          mode: CacheMode::Default,
          manager: CACacheManager::default(),
          options: HttpCacheOptions::default(),
        }))
        .build();
    client
        .get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
        .send()
        .await?;
    Ok(())
}
```

## Streaming Cache Support

For memory-efficient caching of large response bodies, you can use the streaming cache feature. This is particularly useful for handling large files, media content, or API responses without loading the entire response into memory.

To enable streaming cache support, add the `streaming` feature to your `Cargo.toml`:

```toml
[dependencies]
http-cache-reqwest = { version = "1.0", features = ["streaming"] }
```

### Basic Streaming Example

```rust
use http_cache::StreamingManager;
use http_cache_reqwest::StreamingCache;
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use std::path::PathBuf;
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create streaming cache manager
    let cache_manager = StreamingManager::new(PathBuf::from("./cache"), true);
    let streaming_cache = StreamingCache::new(cache_manager);
    
    // Build client with streaming cache
    let client = ClientBuilder::new(Client::new())
        .with(streaming_cache)
        .build();

    // Make request to large content
    let response = client
        .get("https://example.com/large-file.zip")
        .send()
        .await?;

    // Stream the response body
    let mut stream = response.bytes_stream();
    let mut total_bytes = 0;
    
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        total_bytes += chunk.len();
        // Process chunk without loading entire response into memory
    }
    
    println!("Downloaded {total_bytes} bytes");
    Ok(())
}
```

### Key Benefits of Streaming Cache

- **Memory Efficiency**: Large responses are streamed directly to/from disk cache without buffering in memory
- **Performance**: Cached responses can be streamed immediately without waiting for complete download
- **Scalability**: Handle responses of any size without memory constraints
