# http-cache

[![Rust](https://github.com/06chaynes/http-cache/actions/workflows/rust.yml/badge.svg)](https://github.com/06chaynes/http-cache/actions/workflows/rust.yml)
![crates.io](https://img.shields.io/crates/v/http-cache.svg)
[![Docs.rs](https://docs.rs/http-cache/badge.svg)](https://docs.rs/http-cache)

A caching middleware that follows HTTP caching rules, thanks to [http-cache-semantics](https://github.com/kornelski/rusty-http-cache-semantics). By default it uses [cacache](https://github.com/zkat/cacache-rs) as the backend cache manager.

## Supported Clients

- Surf **Should likely be registered after any middleware modifying the request/response*
- Reqwest **Uses [reqwest-middleware](https://github.com/TrueLayer/reqwest-middleware) for middleware support*

## Install

With [cargo add](https://github.com/killercup/cargo-edit#Installation) installed :

```sh
cargo add http-cache
```

## Examples

### Surf (feature: `client-surf`)

```rust
use http_cache::{CACacheManager, Cache, CacheMode};

#[async_std::main]
async fn main() -> surf::Result<()> {
    let req = surf::get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching");
    surf::client()
        .with(Cache {
            mode: CacheMode::Default,
            cache_manager: CACacheManager::default(),
        })
        .send(req)
        .await?;
    Ok(())
}
```

### Reqwest (feature: `client-reqwest`)

```rust
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, Result};
use http_cache::{CACacheManager, Cache, CacheMode};

#[tokio::main]
async fn main() -> Result<()> {
    let client = ClientBuilder::new(Client::new())
        .with(Cache {
            mode: CacheMode::Default,
            cache_manager: CACacheManager::default(),
        })
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
- `client-surf` (disabled): enables [surf](https://github.com/http-rs/surf) client support.
- `client-reqwest` (disabled): enables [reqwest](https://github.com/seanmonstar/reqwest) client support.

## Documentation

- [API Docs](https://docs.rs/http-cache)

## License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
