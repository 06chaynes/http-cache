# http-cache-tower

[![CI](https://img.shields.io/github/actions/workflow/status/06chaynes/http-cache/http-cache-tower.yml?label=CI&style=for-the-badge)](https://github.com/06chaynes/http-cache/actions/workflows/http-cache-tower.yml)
[![Crates.io](https://img.shields.io/crates/v/http-cache-tower?style=for-the-badge)](https://crates.io/crates/http-cache-tower)
[![Docs.rs](https://img.shields.io/docsrs/http-cache-tower?style=for-the-badge)](https://docs.rs/http-cache-tower)
[![Codecov](https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge)](https://app.codecov.io/gh/06chaynes/http-cache)
![Crates.io](https://img.shields.io/crates/l/http-cache-tower?style=for-the-badge)

<img class="logo" align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/main/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">

An HTTP caching middleware for [Tower](https://github.com/tower-rs/tower) and [Hyper](https://hyper.rs/).

This crate provides Tower Layer and Service implementations that add HTTP caching capabilities to your HTTP clients and services.

## Minimum Supported Rust Version (MSRV)

1.88.0

## Install

With [cargo add](https://github.com/killercup/cargo-edit#Installation) installed :

```sh
cargo add http-cache-tower
```

## Features

The following features are available. By default `manager-cacache` is enabled.

- `manager-cacache` (default): enable [cacache](https://github.com/zkat/cacache-rs), a high-performance disk cache, backend manager.
- `manager-moka` (disabled): enable [moka](https://github.com/moka-rs/moka), a high-performance in-memory cache, backend manager.
- `manager-foyer` (disabled): enable [foyer](https://github.com/foyer-rs/foyer), a hybrid in-memory + disk cache, backend manager.
- `streaming` (disabled): enable streaming cache support for memory-efficient handling of large responses using `StreamingManager`.
- `rate-limiting` (disabled): enable cache-aware rate limiting functionality.
- `url-ada` (disabled): enable ada-url for URL parsing.

## Example

### Basic HTTP Cache

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

## Streaming HTTP Cache

For large responses or when memory efficiency is important, use the streaming cache layer:

```rust
# #[cfg(feature = "streaming")]
use http_cache_tower::HttpCacheStreamingLayer;
# #[cfg(feature = "streaming")]
use http_cache::StreamingManager;
# #[cfg(feature = "streaming")]
use tower::{ServiceBuilder, ServiceExt};
# #[cfg(feature = "streaming")]
use http::{Request, Response};
# #[cfg(feature = "streaming")]
use http_body_util::Full;
# #[cfg(feature = "streaming")]
use bytes::Bytes;

# #[cfg(feature = "streaming")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // StreamingManager uses cacache + moka for streaming from disk
    let streaming_manager = StreamingManager::in_memory(1000).await?;

    // Create the streaming cache layer
    let cache_layer = HttpCacheStreamingLayer::new(streaming_manager);

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

# #[cfg(not(feature = "streaming"))]
# fn main() {}
```

**Note**: For memory-efficient streaming of large responses, use `StreamingManager` with `HttpCacheStreamingLayer`. For traditional caching with smaller responses, use `CACacheManager` or `MokaManager` with `HttpCacheLayer`.

## Cache Backends

This crate supports multiple cache backends through feature flags:

- `manager-cacache` (default): Disk-based caching using [cacache](https://github.com/zkat/cacache-rs)
- `manager-moka`: In-memory caching using [moka](https://github.com/moka-rs/moka)

## Integration with Hyper Client

```rust
use http_cache_tower::HttpCacheLayer;
use http_cache::CACacheManager;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use tower::{ServiceBuilder, ServiceExt};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache_manager = CACacheManager::new(PathBuf::from("./cache"), false);
    let cache_layer = HttpCacheLayer::new(cache_manager);

    let client = Client::builder(TokioExecutor::new()).build_http();

    let cached_client = ServiceBuilder::new()
        .layer(cache_layer)
        .service(client);

    // Now use cached_client for HTTP requests
    Ok(())
}
```

## Documentation

- [API Docs](https://docs.rs/http-cache-tower)

## License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
