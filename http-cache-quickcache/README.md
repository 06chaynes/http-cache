# http-cache-quickcache

[![CI](https://img.shields.io/github/actions/workflow/status/06chaynes/http-cache/http-cache-quickcache.yml?label=CI&style=for-the-badge)](https://github.com/06chaynes/http-cache/actions/workflows/http-cache-quickcache.yml)
[![Crates.io](https://img.shields.io/crates/v/http-cache-quickcache?style=for-the-badge)](https://crates.io/crates/http-cache-quickcache)
[![Docs.rs](https://img.shields.io/docsrs/http-cache-quickcache?style=for-the-badge)](https://docs.rs/http-cache-quickcache)
[![Codecov](https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge)](https://app.codecov.io/gh/06chaynes/http-cache)
![Crates.io](https://img.shields.io/crates/l/http-cache-quickcache?style=for-the-badge)

<img class="logo" align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/main/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">

An http-cache manager implementation for [quick-cache](https://github.com/arthurprs/quick-cache).

## Minimum Supported Rust Version (MSRV)

1.82.0

## Install

With [cargo add](https://github.com/killercup/cargo-edit#Installation) installed :

```sh
cargo add http-cache-quickcache
```

## Example

### With Tower Services

```rust
use tower::{Service, ServiceExt};
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use http_cache_quickcache::QuickManager;
use std::convert::Infallible;

// Example Tower service that uses QuickManager for caching
#[derive(Clone)]
struct CachingService {
    cache_manager: QuickManager,
}

impl Service<Request<Full<Bytes>>> for CachingService {
    type Response = Response<Full<Bytes>>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Full<Bytes>>) -> Self::Future {
        let manager = self.cache_manager.clone();
        Box::pin(async move {
            // Cache logic using the manager would go here
            let response = Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::from("Hello from cached service!")))?;
            Ok(response)
        })
    }
}
```

### With Hyper

```rust
use hyper::{Request, Response, StatusCode, body::Incoming};
use http_body_util::Full;
use bytes::Bytes;
use http_cache_quickcache::QuickManager;
use std::convert::Infallible;

async fn handle_request(
    _req: Request<Incoming>,
    cache_manager: QuickManager,
) -> Result<Response<Full<Bytes>>, Infallible> {
    // Use cache_manager here for caching responses
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("cache-control", "max-age=3600")
        .body(Full::new(Bytes::from("Hello from Hyper with caching!")))
        .unwrap())
}
```

## Documentation

- [API Docs](https://docs.rs/http-cache-quickcache)

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
