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

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

## Limiting the cache size

By default the cache keeps every entry until it is invalidated. To limit it, open the database yourself and pass a maximum size in bytes:

```rust
use http_cache::RedbManager;
use redb::Database;
use std::sync::Arc;

let db = Arc::new(Database::create("./http-cache.redb")?);
// Flush every 64 stores (the default) and keep at most 256 MB of entries.
let manager =
    RedbManager::from_database_with_max_bytes(db, 64, 256 * 1024 * 1024)?;
```

When a store takes the cache over the limit, the entries written longest ago are removed in the same transaction. Reading an entry does not count as a use; storing it again, as happens after a revalidation, does. A response larger than the limit on its own is not cached, and is removed if it is already in the database when it is opened.

- The limit counts the serialized entries, not the database file. The file is larger, for three reasons:
  - `redb` stores an entry that does not fit in one page in a power-of-two number of pages, so it can take up to twice its size.
  - Space freed by removing entries that were already flushed can be reused only after the next flush, so the space of up to `flush_interval` stores is held back.
  - While the file is smaller than 4 GiB it at least doubles each time it runs out of space. It shrinks only when at least half of it is free at its end, and then by half of that free space.

  Under steady eviction in our tests the file stopped growing at:

  | Limit | Entry size | Flush interval | File size |
  |-------|-----------|----------------|-----------|
  | 100 MB | 1 to 200 KB | 64, 8 or 1 | 2.57 times the limit |
  | 100 MB | 0.5 to 4 MB | 64 | 5.76 times the limit |
  | 100 MB | 0.5 to 4 MB | 8 | 2.88 times the limit |
  | 100 MB | 0.5 to 4 MB | 1 | 2.16 times the limit |
  | 1 GB | 1 to 200 KB | 64 | 2.01 times the limit |
  | 1 GB | 0.5 to 4 MB | 64 | 2.25 times the limit |

  A lower flush interval reduces the file when entries are large compared to the limit.
- A store that goes over the limit also pays for removing the oldest entries.
- Entries already in the database count towards the limit. The first time a database is opened with a limit they are read once to measure them, and their order is by cache key, as the order they were written in is not known.
- Do not use one database with and without a limit at the same time, and do not let a version of `http-cache` without size limits write to it in between. Entries stored that way are not counted or removed until the manager is opened again, which indexes them in the same way.

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
