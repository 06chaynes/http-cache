# surf

The [`http-cache-surf`](https://github.com/06chaynes/http-cache/tree/main/http-cache-surf) crate provides a [`Middleware`](https://docs.rs/http-cache/latest/http_cache/trait.Middleware.html) implementation for the [`surf`](https://github.com/http-rs/surf) HTTP client.

## Getting Started

```sh
cargo add http-cache-surf
```

## Features

- `manager-cacache`: (default) Enables the [`CACacheManager`](https://docs.rs/http-cache/latest/http_cache/struct.CACacheManager.html) backend cache manager.
- `manager-moka`: Enables the [`MokaManager`](https://docs.rs/http-cache/latest/http_cache/struct.MokaManager.html) backend cache manager.

## Usage

In the following example we will construct our client with our cache struct from [`http-cache-surf`](https://github.com/06chaynes/http-cache/tree/latest/http-cache-surf). This example will use the default mode, default cacache manager, and default http cache options.

After constructing our client, we will make a request to the [MDN Caching Docs](https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching) which should result in an object stored in cache on disk.

```rust
use http_cache_surf::{Cache, CacheMode, CACacheManager, HttpCache, HttpCacheOptions};
use surf::Client;
use macro_rules_attribute::apply;
use smol_macros::main;

#[apply(main!)]
async fn main() -> surf::Result<()> {
    let client = Client::new()
        .with(Cache(HttpCache {
          mode: CacheMode::Default,
          manager: CACacheManager::default(),
          options: HttpCacheOptions::default(),
        }));
    
    client
        .get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
        .await?;
    Ok(())
}
```
