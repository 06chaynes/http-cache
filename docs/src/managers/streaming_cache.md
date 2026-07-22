# StreamingManager (Streaming Cache)

[`StreamingManager`](https://github.com/06chaynes/http-cache/blob/main/http-cache/src/managers/streaming_cache.rs) is a streaming cache manager. Metadata is stored in an embedded [redb](https://github.com/cberner/redb) database; response bodies are written as raw files on disk. [moka](https://github.com/moka-rs/moka) sits in front of redb as an in-memory hot cache of deserialized metadata.

## Key Features

- **True streaming on read**: Cached responses are streamed from disk in 64KB chunks, not loaded fully into memory
- **Persistence across restarts**: redb is an ACID-transactional embedded KV store; cache entries survive process restarts
- **TinyLFU hot-metadata cache**: moka in front of redb uses TinyLFU admission to keep hot metadata warm in RAM
- **Overwrite-crash detection**: every body file is prefixed with a 16-byte nonce stored alongside metadata. On read, the nonce and body length are verified before streaming; a mismatch triggers self-heal
- **True streaming on write**: `put` spools the response frame-by-frame straight to a temp file — at most one frame (typically ~64KB) is ever held in memory, regardless of response size
- **Body size limits**: a configurable maximum cached-body size, enforced as a **decline, not an error** — oversize responses are simply not cached, not rejected
- **Single-instance invariant**: redb's own exclusive file lock on `metadata.redb` prevents two processes from opening the same cache directory at once

## Write-Path: Bounded-Memory Spooling

Both directions are now streaming: cached responses are streamed on **read** (GET) in 64KB chunks, and the **write** path (PUT) spools the incoming body frame-by-frame to a temp file, flushing each frame before the next is pulled — bounded RAM of roughly one frame regardless of response size. The tmp file is `fsync`ed, then atomically renamed into place before the redb metadata transaction commits. On success, `put` returns a disk-backed, checksum-verified `File` body served from the just-committed file, not an in-memory copy.

`max_body_size` bounds the size of entries this manager will *cache*, not the memory used while writing them:

- A declared `Content-Length` over the limit skips the spool entirely — nothing touches disk, and the response streams straight through to the caller uncached (HEAD responses are exempt from this check: their `Content-Length` describes the entity, not the empty body actually sent).
- If the length isn't known up front and the spooled byte count crosses the limit mid-stream, caching is declined at that point and the remainder of the body is streamed straight through — the caller still gets the complete response, it just isn't cached.
- If the received byte count ends up not matching a declared `Content-Length`, the response is still served to the caller but is not cached (RFC 9111 §3.3 — never store a response known to be incomplete; HEAD exempt).

None of these cases fail the request: a broken or over-limit cache write degrades to "serve uncached," never to an error. The default limit is 100MB.

## Getting Started

The `StreamingManager` is built into the core `http-cache` crate and is available when the `streaming` feature is enabled.

```toml
[dependencies]
http-cache = { version = "1.0", features = ["streaming"] }
```

## Basic Usage

```rust
use http_cache::StreamingManager;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a streaming cache manager with disk storage
    let manager = StreamingManager::new(
        PathBuf::from("./cache"),  // Cache directory
        10_000,                     // Moka hot-cache capacity (warm metadata entries)
    ).await?;

    // Or use with_temp_dir() for testing (uses temp directory)
    let test_manager = StreamingManager::with_temp_dir(1000).await?;

    // For custom body size limits (e.g., 50MB max)
    let custom_manager = StreamingManager::with_max_body_size(
        PathBuf::from("./cache"),
        10_000,
        50 * 1024 * 1024,
    ).await?;

    Ok(())
}
```

## Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│  moka::Cache<String, CacheMetadata>  (in-memory hot cache)      │
│  - TinyLFU admission; capacity bounds RAM use                   │
│  - Eviction drops RAM only — disk state is unaffected           │
└─────────────────────────────────────────────────────────────────┘
                          │  miss → fall through
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  redb (metadata.redb)                                           │
│  - ACID-transactional embedded B-tree KV store                  │
│  - key → postcard(CacheMetadata) { status, headers, policy,    │
│                                     body_size, nonce, ... }     │
│  - Exclusive file lock (single-writer per cache_dir)            │
└─────────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  bodies/<prefix>/<blake3(key)>.bin   (raw files)                │
│  - Format: [16-byte nonce][body bytes]                          │
│  - Streaming reads via tokio::fs::File (AsyncRead, 64KB buffer) │
│  - Overwrites atomically replace prior content (rename)         │
└─────────────────────────────────────────────────────────────────┘
```

## On-Disk Layout

```text
$cache_dir/
├── metadata.redb           # redb database (key → postcard-encoded metadata)
├── bodies/                 # body files, sharded by blake3(key) prefix
│   ├── 0a/
│   │   └── 0a1b2c3d….bin   # [16-byte nonce][body bytes]
│   └── …
└── tmp/                    # staging directory for in-flight body writes
```

The `body_hash = blake3(cache_key)` is deterministic from the cache key, so overwrites to the same key target the same final path — the atomic rename replaces prior content without leaving orphans.

## Crash Safety

Put ordering is strictly:

1. Write `[16-byte nonce][body bytes]` to `tmp/<body_hash>.<rand>.tmp` (the tmp suffix is a random `u64` distinct from the crash-detection nonce, used only to prevent tmp-file collisions between concurrent puts)
2. `fsync` the tmp file
3. Atomic `rename` → `bodies/<prefix>/<body_hash>.bin`
4. Commit the redb metadata transaction
5. Update moka hot cache

If a crash occurs mid-sequence:

- Before step 3: `.tmp` file is swept on next startup
- After step 3 but before step 4: for a new key, metadata is absent so `get` returns `None` (body is orphaned, reclaimable by a future GC); for an overwrite, the body file has been replaced but redb holds old metadata — the nonce mismatch on the next `get` triggers self-heal
- After step 4: disk state is consistent

The 16-byte nonce written as the file header is regenerated per-put and compared to the nonce stored in `CacheMetadata` on every `get`. A mismatch (indicating the body file was replaced without the metadata commit landing) causes `get` to remove the redb row, invalidate moka, and return `Ok(None)`.

## Single-Instance Invariant

Only one `StreamingManager` may point at a given cache directory at a time. Construction of a second instance against the same `cache_dir` while the first is alive returns an error — enforced by redb's exclusive file lock on `metadata.redb`.

This is a local-filesystem guarantee. Advisory file locks are unreliable on NFS, some container overlay filesystems, and some exotic setups — do not share a cache directory across hosts.

## Memory Efficiency

**On cache hit (GET):** only ~64KB is held in memory at a time (the streaming buffer), regardless of response size:

| Response Size | Peak Memory (Buffered) | Peak Memory (Streaming GET) |
|---------------|------------------------|-----------------------------|
| 100KB | 100KB | ~64KB |
| 1MB | 1MB | ~64KB |
| 10MB | 10MB | ~64KB |
| 100MB | 100MB | ~64KB |

**On cache write (PUT):** only one frame (typically ~64KB) is held in memory at a time — the body is spooled frame-by-frame straight to a temp file, flushed per frame, then atomically renamed into place. `max_body_size` (default: 100MB) bounds which entries get cached, not write-path memory: responses over the limit are simply not cached (decline, not error) — the caller still receives the full body streamed through.

## Usage with Tower

```rust
use http_cache::StreamingManager;
use http_cache_tower::HttpCacheStreamingLayer;
use tower::{Service, ServiceExt};
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = StreamingManager::new(
        PathBuf::from("./cache"),
        10_000,
    ).await?;

    let cache_layer = HttpCacheStreamingLayer::new(manager);

    let service = tower::service_fn(|_req: Request<Full<Bytes>>| async {
        Ok::<_, std::convert::Infallible>(
            Response::builder()
                .status(StatusCode::OK)
                .header("cache-control", "max-age=3600")
                .body(Full::new(Bytes::from("Response data...")))?
        )
    });

    let cached_service = cache_layer.layer(service);

    let request = Request::builder()
        .uri("https://example.com/api")
        .body(Full::new(Bytes::new()))?;

    let response = cached_service.oneshot(request).await?;
    println!("Response status: {}", response.status());

    Ok(())
}
```

## Usage with Reqwest

```rust
use http_cache::StreamingManager;
use http_cache_reqwest::{StreamingCache, CacheMode};
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = StreamingManager::new(
        PathBuf::from("./cache"),
        10_000,
    ).await?;

    let client = ClientBuilder::new(Client::new())
        .with(StreamingCache::new(manager, CacheMode::Default))
        .build();

    let response = client
        .get("https://httpbin.org/get")
        .send()
        .await?;

    println!("Status: {}", response.status());
    Ok(())
}
```

## Working with the manager directly

### Caching a response

```rust
use http_cache::{StreamingManager, StreamingCacheManager};
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use http_cache_semantics::CachePolicy;
use url::Url;
use std::path::PathBuf;

let manager = StreamingManager::new(PathBuf::from("./cache"), 10_000).await?;

let response = Response::builder()
    .status(StatusCode::OK)
    .header("cache-control", "max-age=3600, public")
    .header("content-type", "application/json")
    .body(Full::new(Bytes::from(r#"{"data": "example"}"#)))?;

let request = Request::builder()
    .method("GET")
    .uri("https://example.com/api")
    .body(())?;
let policy = CachePolicy::new(&request, &Response::builder()
    .status(200)
    .header("cache-control", "max-age=3600, public")
    .body(vec![])?);

let url = Url::parse("https://example.com/api")?;
let cached_response = manager.put(
    "GET:https://example.com/api".to_string(),
    response,
    policy,
    url,
    None, // optional metadata
).await?;
```

### Retrieving a cached response

```rust
// Retrieve from cache — body is streamed from disk
let cached = manager.get("GET:https://example.com/api").await?;

if let Some((response, policy)) = cached {
    println!("Cache hit! Status: {}", response.status());

    // The body streams from the on-disk file; memory stays bounded
    use http_body_util::BodyExt;
    let body = response.into_body();
    let bytes = body.collect().await?.to_bytes();
    println!("Body: {} bytes", bytes.len());
} else {
    println!("Cache miss");
}
```

### Updating metadata on revalidation (304)

When a conditional request comes back `304 Not Modified`, the body on disk is
still valid — only the stored headers, cache policy, and user metadata need
refreshing. `update_metadata` does exactly that, without ever re-reading or
re-writing the body file:

```rust
use http_cache::CacheEntryToken;

// A `CacheEntryToken`, when supplied, ties this update to the exact entry
// that a prior `get()` read (it arrives as a response extension on that
// call's `Response<Self::Body>`). Passing `None` skips that check.
let token: Option<CacheEntryToken> = None;

let updated = manager.update_metadata(
    "GET:https://example.com/api",
    &new_headers,
    new_policy,
    None, // user metadata, unchanged here
    token.as_ref(),
).await?;

if !updated {
    // Entry was deleted or replaced concurrently — serve the fresh
    // response as-is and skip re-caching.
}
```

### Deleting cached entries

```rust
manager.delete("GET:https://example.com/api").await?;
```

### Cache management

```rust
// Number of entries currently warm in the in-memory hot cache.
// NOT the total number of entries persisted to disk — cold entries
// remain reachable via `get` but are not counted here.
let warm_count = manager.entry_count();

// Clear all entries (moka, redb, and on-disk bodies)
manager.clear().await?;

// Run pending moka maintenance tasks
manager.run_pending_tasks().await;
```

## No Content Deduplication

Unlike the previous cacache-based implementation, this design stores a separate body file per cache key. Two different keys with identical bodies produce two separate files on disk. This is an acceptable tradeoff for an HTTP cache, where distinct URLs almost always yield distinct bodies, in exchange for simpler semantics: overwrites to the same key atomically replace prior content without orphan tracking.

## Comparison with Buffered Caching

| Aspect | CACacheManager (Buffered) | StreamingManager |
|--------|---------------------------|------------------|
| **Memory on GET** | Full body in memory | ~64KB streaming buffer |
| **Memory on PUT** | Full body in memory | ~1 frame (typically 64KB), regardless of body size |
| **Persistence** | Yes | Yes (redb) |
| **Hot-cache eviction** | None | TinyLFU (moka) for metadata only; disk state unaffected |
| **Content dedup** | Yes (cacache) | No (per-key body files) |
| **Body size limit** | None | Configurable (default 100MB); over-limit responses decline to cache, not an error |
| **Multi-process** | Supported via cacache | Single-instance per cache_dir (redb lock) |
| **Use case** | Small responses, dedup-heavy workloads | Large responses, long-lived caches |
