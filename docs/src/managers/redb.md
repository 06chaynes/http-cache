# redb

[`redb`](https://github.com/cberner/redb) is a simple, portable, pure-Rust embedded key/value database. The `http-cache` implementation, `RedbManager`, provides traditional buffered, **persistent** caching that depends on **no async runtime** — `redb` itself only depends on `libc`.

As of this version, `RedbManager` is the **default** cache manager for `http-cache` and its client adapters.

This makes `RedbManager` the recommended persistent backend for clients that do **not** run on tokio — for example the smol-based [`http-cache-ureq`](../clients/ureq.md) adapter, or runtimes such as Bevy. Unlike `CACacheManager` (which is built on `cacache`'s tokio runtime), `RedbManager` pulls in no `tokio` dependency and requires no reactor at runtime.

## Getting Started

The `redb` backend cache manager is available when the `manager-redb` feature is enabled.

```toml
[dependencies]
http-cache = { version = "1.0", features = ["manager-redb"] }
```

## Basic Usage

`RedbManager` stores the entire cache in a single database file:

```rust
use http_cache::RedbManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Creates or opens the database file at the given path.
    let manager = RedbManager::new("./http-cache.redb")?;

    Ok(())
}
```

No async runtime is required, so it works under any executor (tokio, smol, Bevy, …).

## Features

- **Persistent**: Cached entries survive process restarts.
- **No async runtime**: All I/O is synchronous; no tokio (or other runtime) dependency, and no reactor needed at runtime.
- **Pure Rust**: `redb` depends only on `libc`, avoiding heavier transitive dependencies.

## Working with the Manager Directly

The `RedbManager` implements the `CacheManager` trait:

```rust
use http_cache::{CacheManager, HttpResponse, HttpVersion, RedbManager};
use http_cache_semantics::CachePolicy;
use url::Url;

let manager = RedbManager::new("./http-cache.redb")?;

// Store a response in the cache
let url = Url::parse("http://example.com")?;
let response = HttpResponse {
    body: b"response body".to_vec(),
    headers: Default::default(),
    status: 200,
    url: url.clone(),
    version: HttpVersion::Http11,
    metadata: None,
};
let req = http::Request::get("http://example.com").body(())?;
let res = http::Response::builder()
    .status(200)
    .body(b"response body".to_vec())?;
let policy = CachePolicy::new(&req, &res);
let cached = manager.put("my-cache-key".into(), response, policy).await?;

// Retrieve a cached response
let response = manager.get("my-cache-key").await?;

// Remove from cache
manager.delete("my-cache-key").await?;

// Clear the entire cache
manager.clear().await?;
```

## Single-instance invariant

`redb` takes an exclusive lock on the database file, so only one `RedbManager`
may have a given file open at a time. Share a single instance (e.g. behind an
`Arc` or a `static`) rather than constructing a second manager for the same
path.

## A note on blocking I/O

`redb` operations are synchronous and run inline on the task that awaits them
(there is no offload to a blocking thread pool). For a local, embedded
database this is typically very fast; on tokio-native adapters prefer a
multi-threaded runtime.

## When to Use redb

`RedbManager` is ideal for:

- **Non-tokio clients**: smol-based clients (`http-cache-ureq`) or runtimes like Bevy that have no tokio reactor.
- **Persistence without tokio**: when cached data should survive restarts but you want no async-runtime dependency.

For other use cases, consider:
- `CACacheManager` for content-addressed disk caching on tokio.
- `MokaManager` / `QuickManager` for in-memory caching.
- `FoyerManager` for a hybrid memory + disk cache on tokio.
