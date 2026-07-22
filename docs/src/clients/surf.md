# surf

The [`http-cache-surf`](https://github.com/06chaynes/http-cache/tree/main/http-cache-surf) crate provides a [`Middleware`](https://docs.rs/http-cache/latest/http_cache/trait.Middleware.html) implementation for the [`surf`](https://github.com/http-rs/surf) HTTP client.

## Getting Started

```sh
cargo add http-cache-surf
```

## Features

- `manager-redb`: (default) Enables the [`RedbManager`](https://docs.rs/http-cache/latest/http_cache/struct.RedbManager.html) backend cache manager.
- `manager-cacache`: Enables the [`CACacheManager`](https://docs.rs/http-cache/latest/http_cache/struct.CACacheManager.html) backend cache manager.
- `manager-moka`: Enables the [`MokaManager`](https://docs.rs/http-cache/latest/http_cache/struct.MokaManager.html) backend cache manager.
- `manager-foyer`: Enables the [`FoyerManager`](https://docs.rs/http-cache/latest/http_cache/struct.FoyerManager.html) backend cache manager.
- `rate-limiting`: Enables cache-aware rate limiting functionality.
- `url-ada`: Enables ada-url for URL parsing (mutually exclusive with the default `url-standard`).

## Usage

In the following example we will construct our client with our cache struct from [`http-cache-surf`](https://github.com/06chaynes/http-cache/tree/main/http-cache-surf). This example will use the default mode, default redb manager, and default http cache options.

After constructing our client, we will make a request to the [MDN Caching Docs](https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching) which should result in an object stored in cache on disk.

```rust
use http_cache_surf::{Cache, CacheMode, RedbManager, HttpCache, HttpCacheOptions};
use surf::Client;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::new()
        .with(Cache(HttpCache {
          mode: CacheMode::Default,
          manager: RedbManager::new("./http-cache.redb")?,
          options: HttpCacheOptions::default(),
        }));

    client
        .get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
        .await?;
    Ok(())
}
```
