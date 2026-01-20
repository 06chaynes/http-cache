# http-cache-ureq

[![CI](https://img.shields.io/github/actions/workflow/status/06chaynes/http-cache/http-cache-ureq.yml?label=CI&style=for-the-badge)](https://github.com/06chaynes/http-cache/actions/workflows/http-cache-ureq.yml)
[![Crates.io](https://img.shields.io/crates/v/http-cache-ureq?style=for-the-badge)](https://crates.io/crates/http-cache-ureq)
[![Docs.rs](https://img.shields.io/docsrs/http-cache-ureq?style=for-the-badge)](https://docs.rs/http-cache-ureq)
[![Codecov](https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge)](https://app.codecov.io/gh/06chaynes/http-cache)
![Crates.io](https://img.shields.io/crates/l/http-cache-ureq?style=for-the-badge)

<img class="logo" align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/main/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">

A caching middleware that follows HTTP caching rules,
thanks to [http-cache-semantics](https://github.com/kornelski/rusty-http-cache-semantics).
By default, it uses [cacache](https://github.com/zkat/cacache-rs) as the backend cache manager.
Provides a simple caching wrapper around [ureq](https://github.com/algesten/ureq).

## Minimum Supported Rust Version (MSRV)

1.85.0

## Install

With [cargo add](https://github.com/killercup/cargo-edit#Installation) installed :

```sh
cargo add http-cache-ureq
```

## Example

```rust
use http_cache_ureq::{CACacheManager, CachedAgent};
use std::path::PathBuf;

#[smol_macros::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = CachedAgent::builder()
        .cache_manager(CACacheManager::new(PathBuf::from("./cache"), false))
        .build()?;

    let response = client
        .get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
        .call()
        .await?;

    println!("Status: {}", response.status());
    Ok(())
}
```

## Basic Usage

The `CachedAgent` wraps ureq's functionality while providing transparent HTTP caching:

```rust
use http_cache_ureq::{CACacheManager, CachedAgent};
use std::path::PathBuf;

// Create a cached agent with default settings
let client = CachedAgent::builder()
    .cache_manager(CACacheManager::new(PathBuf::from("./cache"), false))
    .build()?;

// Use it just like a regular ureq agent
let response = client.get("https://httpbin.org/json").call().await?;
```

## Features

The following features are available. By default `manager-cacache` is enabled.

- `manager-cacache` (default): enable [cacache](https://github.com/zkat/cacache-rs), a high-performance disk cache, backend manager.
- `manager-moka` (disabled): enable [moka](https://github.com/moka-rs/moka), a high-performance in-memory cache, backend manager.
- `manager-foyer` (disabled): enable [foyer](https://github.com/foyer-rs/foyer), a hybrid in-memory + disk cache, backend manager.
- `json` (disabled): enable JSON support via ureq's json feature.
- `rate-limiting` (disabled): enable cache-aware rate limiting functionality.
- `url-ada` (disabled): enable ada-url for URL parsing.

## Documentation

- [API Docs](https://docs.rs/http-cache-ureq)

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
