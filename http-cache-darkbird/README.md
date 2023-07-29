# http-cache-darkbird

[![CI](https://img.shields.io/github/actions/workflow/status/06chaynes/http-cache/http-cache-darkbird.yml?label=CI&style=for-the-badge)](https://github.com/06chaynes/http-cache/actions/workflows/http-cache-darkbird.yml)
[![Crates.io](https://img.shields.io/crates/v/http-cache-darkbird?style=for-the-badge)](https://crates.io/crates/http-cache-darkbird)
[![Docs.rs](https://img.shields.io/docsrs/http-cache-darkbird?style=for-the-badge)](https://docs.rs/http-cache-darkbird)
[![Codecov](https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge)](https://app.codecov.io/gh/06chaynes/http-cache)
![Crates.io](https://img.shields.io/crates/l/http-cache-darkbird?style=for-the-badge)

<img class="logo" align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/main/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">

An http-cache manager implementation for [darkbird](https://github.com/Rustixir/darkbird).

## Minimum Supported Rust Version (MSRV)

1.66.1

## Install

With [cargo add](https://github.com/killercup/cargo-edit#Installation) installed :

```sh
cargo add http-cache-darkbird
```

## Example

```rust
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, Result};
use http_cache_reqwest::{Cache, CacheMode, HttpCache, HttpCacheOptions};
use http_cache_darkbird::DarkbirdManager;

#[tokio::main]
async fn main() -> Result<()> {
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
          mode: CacheMode::Default,
          manager: DarkbirdManager::new_with_defaults().await?,
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

## Documentation

- [API Docs](https://docs.rs/http-cache-darkbird)

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
