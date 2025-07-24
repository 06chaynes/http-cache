# tower

The [`http-cache-tower`](https://github.com/06chaynes/http-cache/tree/main/http-cache-tower) crate provides Tower Layer and Service implementations that add HTTP caching capabilities to your HTTP clients and services. It supports both regular and **full streaming cache operations** for memory-efficient handling of large responses.

## Getting Started

```sh
cargo add http-cache-tower
```

## Features

- `manager-cacache`: (default) Enables the [`CACacheManager`](https://docs.rs/http-cache/latest/http_cache/struct.CACacheManager.html) backend cache manager.
- `manager-moka`: Enables the [`MokaManager`](https://docs.rs/http-cache/latest/http_cache/struct.MokaManager.html) backend cache manager.

## Basic Usage

Here's a basic example using the regular HTTP cache layer:

```rust
use http_cache_tower::HttpCacheLayer;
use http_cache::CACacheManager;
use tower::{ServiceBuilder, ServiceExt};
use http::{Request, Response};
use http_body_util::Full;
use bytes::Bytes;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a cache manager
    let cache_manager = CACacheManager::new(PathBuf::from("./cache"), false);

    // Create the cache layer
    let cache_layer = HttpCacheLayer::new(cache_manager);

    // Build your service stack
    let service = ServiceBuilder::new()
        .layer(cache_layer)
        .service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::convert::Infallible>(
                Response::new(Full::new(Bytes::from("Hello, world!")))
            )
        });

    // Use the service
    let request = Request::builder()
        .uri("https://httpbin.org/cache/300")
        .body(Full::new(Bytes::new()))?;

    let response = service.oneshot(request).await?;

    println!("Status: {}", response.status());

    Ok(())
}
```

## Streaming Usage

For large responses or when memory efficiency is important, use the streaming cache layer:

```rust
use http_cache_tower::{HttpCacheStreamingLayer, StreamingCacheWrapper};
use http_cache::CACacheManager;
use tower::{ServiceBuilder, ServiceExt};
use http::{Request, Response};
use http_body_util::Full;
use bytes::Bytes;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a streaming cache manager
    let cache_manager = CACacheManager::new(PathBuf::from("./cache"), false);
    let streaming_wrapper = StreamingCacheWrapper::new(cache_manager);

    // Create the streaming cache layer
    let cache_layer = HttpCacheStreamingLayer::new(streaming_wrapper);

    // Build your service stack
    let service = ServiceBuilder::new()
        .layer(cache_layer)
        .service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::convert::Infallible>(
                Response::new(Full::new(Bytes::from("Large response data...")))
            )
        });

    // Use the service - responses are streamed without buffering entire body
    let request = Request::builder()
        .uri("https://example.com/large-file")
        .body(Full::new(Bytes::new()))?;

    let response = service.oneshot(request).await?;

    println!("Status: {}", response.status());

    Ok(())
}
```

## Integration with Hyper Client

The tower layers can be easily integrated with Hyper clients:

```rust
use http_cache_tower::HttpCacheLayer;
use http_cache::CACacheManager;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use tower::{ServiceBuilder, ServiceExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache_manager = CACacheManager::default();
    let cache_layer = HttpCacheLayer::new(cache_manager);

    let client = Client::builder(TokioExecutor::new()).build_http();

    let cached_client = ServiceBuilder::new()
        .layer(cache_layer)
        .service(client);

    // Now use cached_client for HTTP requests
    Ok(())
}
```
