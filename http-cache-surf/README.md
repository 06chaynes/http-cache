# http-cache-surf

[![Rust](https://github.com/06chaynes/http-cache/actions/workflows/rust.yml/badge.svg)](https://github.com/06chaynes/http-cache/actions/workflows/rust.yml)
![crates.io](https://img.shields.io/crates/v/http-cache-surf.svg)
[![Docs.rs](https://docs.rs/http-cache-surf/badge.svg)](https://docs.rs/http-cache-surf)

<img align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/latest/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">

A caching middleware that follows HTTP caching rules,
thanks to [http-cache-semantics](https://github.com/kornelski/rusty-http-cache-semantics).
By default, it uses [cacache](https://github.com/zkat/cacache-rs) as the backend cache manager.
Should likely be registered after any middleware modifying the request.

## Install

With [cargo add](https://github.com/killercup/cargo-edit#Installation) installed :

```sh
cargo add http-cache-surf
```

## Example

```rust
use http_cache_surf::{Cache, CacheMode, CACacheManager, HttpCache};

#[async_std::main]
async fn main() -> surf::Result<()> {
    let req = surf::get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching");
    surf::client()
        .with(Cache::new(HttpCache {
          mode: CacheMode::Default,
          manager: CACacheManager::default(),
          options: None,
        }))
        .send(req)
        .await?;
    Ok(())
}
```

## Features

The following features are available. By default `manager-cacache` is enabled.

- `manager-cacache` (default): use [cacache](https://github.com/zkat/cacache-rs), a high-performance disk cache, for the manager backend.

## Documentation

- [API Docs](https://docs.rs/http-cache-surf)

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
