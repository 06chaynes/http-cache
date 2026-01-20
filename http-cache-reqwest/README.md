# http-cache-reqwest

[![CI](https://img.shields.io/github/actions/workflow/status/06chaynes/http-cache/http-cache-reqwest.yml?label=CI&style=for-the-badge)](https://github.com/06chaynes/http-cache/actions/workflows/http-cache-reqwest.yml)
[![Crates.io](https://img.shields.io/crates/v/http-cache-reqwest?style=for-the-badge)](https://crates.io/crates/http-cache-reqwest)
[![Docs.rs](https://img.shields.io/docsrs/http-cache-reqwest?style=for-the-badge)](https://docs.rs/http-cache-reqwest)
[![Codecov](https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge)](https://app.codecov.io/gh/06chaynes/http-cache)
![Crates.io](https://img.shields.io/crates/l/http-cache-reqwest?style=for-the-badge)

<img class="logo" align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/main/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">

A caching middleware that follows HTTP caching rules,
thanks to [http-cache-semantics](https://github.com/kornelski/rusty-http-cache-semantics).
By default, it uses [cacache](https://github.com/zkat/cacache-rs) as the backend cache manager.
Uses [reqwest-middleware](https://github.com/TrueLayer/reqwest-middleware) for middleware support.

## Minimum Supported Rust Version (MSRV)

1.85.0

## Install

With [cargo add](https://github.com/killercup/cargo-edit#Installation) installed :

```sh
cargo add http-cache-reqwest
````

## Example

```rust
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, Result};
use http_cache_reqwest::{Cache, CacheMode, CACacheManager, HttpCache, HttpCacheOptions};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
          mode: CacheMode::Default,
          manager: CACacheManager::new(PathBuf::from("./cache"), false),
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

## Streaming Support

When the `streaming` feature is enabled, you can use `StreamingCache` for efficient handling of large responses without buffering them entirely in memory. This provides significant memory savings (typically 35-40% reduction) while maintaining full HTTP caching compliance.

**Note**: Only `StreamingCacheManager` supports streaming. `CACacheManager` and `MokaManager` do not support streaming and will buffer responses in memory.

```rust
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use http_cache_reqwest::{StreamingCache, CacheMode};

#[cfg(feature = "streaming")]
use http_cache::StreamingManager;
use std::path::PathBuf;

#[cfg(feature = "streaming")]
#[tokio::main]
async fn main() -> reqwest_middleware::Result<()> {
    let client = ClientBuilder::new(Client::new())
        .with(StreamingCache::new(
            StreamingManager::new(PathBuf::from("./cache")),
            CacheMode::Default,
        ))
        .build();
        
    // Efficiently stream large responses - cached responses are also streamed
    let response = client
        .get("https://httpbin.org/stream/1000")
        .send()
        .await?;
        
    // Process response as a stream
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        // Process each chunk without loading entire response into memory
        println!("Received {} bytes", chunk.len());
    }
    
    Ok(())
}

#[cfg(not(feature = "streaming"))]
fn main() {}
```

## Features

The following features are available. By default `manager-cacache` is enabled.

- `manager-cacache` (default): enable [cacache](https://github.com/zkat/cacache-rs), a high-performance disk cache, backend manager.
- `manager-moka` (disabled): enable [moka](https://github.com/moka-rs/moka), a high-performance in-memory cache, backend manager.
- `manager-foyer` (disabled): enable [foyer](https://github.com/foyer-rs/foyer), a hybrid in-memory + disk cache, backend manager.
- `streaming` (disabled): enable streaming cache support with efficient memory usage. Provides `StreamingCache` middleware that can handle large responses without buffering them entirely in memory, while maintaining full HTTP caching compliance. Requires cache managers that implement `StreamingCacheManager`.
- `rate-limiting` (disabled): enable cache-aware rate limiting functionality.
- `url-ada` (disabled): enable ada-url for URL parsing.

## Documentation

- [API Docs](https://docs.rs/http-cache-reqwest)

## License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](https://github.com/06chaynes/http-cache/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license
  ([LICENSE-MIT](https://github.com/06chaynes/http-cache/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
