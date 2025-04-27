# http-cache-mokadeser

[![CI](https://img.shields.io/github/actions/workflow/status/06chaynes/http-cache/http-cache-mokadeser.yml?label=CI&style=for-the-badge)](https://github.com/06chaynes/http-cache/actions/workflows/http-cache-mokadeser.yml)
[![Crates.io](https://img.shields.io/crates/v/http-cache-mokadeser?style=for-the-badge)](https://crates.io/crates/http-cache-mokadeser)
[![Docs.rs](https://img.shields.io/docsrs/http-cache-mokadeser?style=for-the-badge)](https://docs.rs/http-cache-mokadeser)
[![Codecov](https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge)](https://app.codecov.io/gh/06chaynes/http-cache)
![Crates.io](https://img.shields.io/crates/l/http-cache-mokadeser?style=for-the-badge)

<img class="logo" align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/main/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">

An http-cache manager implementation for [moka](https://github.com/moka-rs/moka).

## Minimum Supported Rust Version (MSRV)

1.81.0

## Install

With [cargo add](https://github.com/killercup/cargo-edit#Installation) installed :

```sh
cargo add http-cache-mokadeser
```

## Example

```rust
use http_cache_mokadeser::MokaManager;
use http_cache_surf::{Cache, CacheMode, HttpCache, HttpCacheOptions};

#[async_std::main]
async fn main() -> surf::Result<()> {
    let req = surf::get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching");
    surf::client()
        .with(Cache(HttpCache {
          mode: CacheMode::Default,
          manager: MokaManager::default(),
          options: HttpCacheOptions::default(),
        }))
        .send(req)
        .await?;
    Ok(())
}
```

## Documentation

- [API Docs](https://docs.rs/http-cache-mokadeser)

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
