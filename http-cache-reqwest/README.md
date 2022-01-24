# http-cache-reqwest

[![CI](https://img.shields.io/github/workflow/status/06chaynes/http-cache/Rust?label=CI&style=for-the-badge)](https://github.com/06chaynes/http-cache/actions/workflows/rust.yml)
[![Crates.io](https://img.shields.io/crates/v/http-cache-reqwest?style=for-the-badge)](https://crates.io/crates/http-cache-reqwest)
[![Docs.rs](https://img.shields.io/docsrs/http-cache-reqwest?style=for-the-badge)](https://docs.rs/http-cache-reqwest)
[![Codecov](https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge)](https://app.codecov.io/gh/06chaynes/http-cache)
![Crates.io](https://img.shields.io/crates/l/http-cache-reqwest?style=for-the-badge)

<img align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/latest/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">

A caching middleware that follows HTTP caching rules,
thanks to [http-cache-semantics](https://github.com/kornelski/rusty-http-cache-semantics).
By default, it uses [cacache](https://github.com/zkat/cacache-rs) as the backend cache manager.
Uses [reqwest-middleware](https://github.com/TrueLayer/reqwest-middleware) for middleware support.

## Minimum Supported Rust Version (MSRV)

1.54.0

## Install

With [cargo add](https://github.com/killercup/cargo-edit#Installation) installed :

```sh
cargo add http-cache-reqwest
````

## Example

```rust
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, Result};
use http_cache_reqwest::{Cache, CacheMode, CACacheManager, HttpCache};

#[tokio::main]
async fn main() -> Result<()> {
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
          mode: CacheMode::Default,
          manager: CACacheManager::default(),
          options: None,
        }))
        .build();
    client
        .get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
        .send()
        .await?;
    Ok(())
}
```

## Features

The following features are available. By default `manager-cacache` is enabled.

- `manager-cacache` (default): use [cacache](https://github.com/zkat/cacache-rs), a high-performance disk cache, for the manager backend.
- `manager-moka` (disabled): use [moka](https://github.com/moka-rs/moka), a high-performance in-memory cache, for the manager backend.

## Documentation

- [API Docs](https://docs.rs/http-cache-reqwest)

## License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license
  ([LICENSE-MIT](../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
