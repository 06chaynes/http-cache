# reqwest

The [`http-cache-reqwest`](https://github.com/06chaynes/http-cache/tree/main/http-cache-reqwest) crate provides a [`Middleware`](https://docs.rs/http-cache/latest/http_cache/trait.Middleware.html) implementation for the [`reqwest`](https://github.com/seanmonstar/reqwest) HTTP client. It accomplishes this by utilizing [`reqwest_middleware`](https://github.com/TrueLayer/reqwest-middleware).

## Getting Started

```sh
cargo add http-cache-reqwest
```

## Features

- `manager-cacache`: (default) Enables the [`CACacheManager`](https://docs.rs/http-cache/latest/http_cache/struct.CACacheManager.html) backend cache manager.
- `manager-moka`: Enables the [`MokaManager`](https://docs.rs/http-cache/latest/http_cache/struct.MokaManager.html) backend cache manager.

## Usage

In the following example we will construct our client using the builder provided by [`reqwest_middleware`](https://github.com/TrueLayer/reqwest-middleware) with our cache struct from [`http-cache-reqwest`](https://github.com/06chaynes/http-cache/tree/latest/http-cache-reqwest). This example will use the default mode, default cacache manager, and default http cache options.

After constructing our client, we will make a request to the [MDN Caching Docs](https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching) which should result in an object stored in cache on disk.

```rust
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, Result};
use http_cache_reqwest::{Cache, CacheMode, CACacheManager, HttpCache, HttpCacheOptions};

#[tokio::main]
async fn main() -> Result<()> {
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
          mode: CacheMode::Default,
          manager: CACacheManager::default(),
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
