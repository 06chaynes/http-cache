# Backend Cache Manager Implementations

The following backend cache manager implementations are provided by this crate:

## Size limits

No manager removes entries because of their age, and `max_ttl` only limits how long a response is fresh. Whether a cache stops growing depends on the manager:

| Manager | Limited by | How |
|---------|------------|-----|
| [moka](./moka.md) | entries or bytes | `Cache::new(entries)`, or a [weigher](./moka.md#limiting-the-cache-by-size) |
| [quick_cache](./quick-cache.md) | entries | `Cache::new(entries)` |
| [foyer](./foyer.md) | memory and disk capacity | `HybridCacheBuilder::memory` and `FsDeviceBuilder::with_capacity` |
| [redb](./redb.md) | bytes, when asked | [`RedbManager::from_database_with_max_bytes`](./redb.md#limiting-the-cache-size) |
| [cacache](./cacache.md) | nothing | Remove entries yourself, for example by the `size` and `time` that `cacache::list_sync` reports |
| [streaming_cache](./streaming_cache.md) | bytes on disk, when asked | [`StreamingManagerOptions::max_bytes`](./streaming_cache.md#limiting-the-disk-size); `capacity` limits the in-memory metadata only |

## [cacache](./cacache.md)

[`cacache`](https://github.com/zkat/cacache-rs) is a high-performance, concurrent, content-addressable disk cache, optimized for async APIs. Provides traditional buffered caching. Now **opt-in** (no longer the default); its upstream is dormant/unmaintained.

## [moka](./moka.md)

[`moka`](https://github.com/moka-rs/moka) is a fast, concurrent cache library inspired by the Caffeine library for Java. Provides in-memory caching with traditional buffering.

## [foyer](./foyer.md)

[`foyer`](https://github.com/foyer-rs/foyer) is a hybrid in-memory + disk cache that provides configurable eviction strategies (w-TinyLFU, S3-FIFO, SIEVE), optional disk storage, request deduplication, and Tokio-native async operations.

## [redb](./redb.md)

[`redb`](https://github.com/cberner/redb) is a simple, portable, pure-Rust embedded key/value database. `RedbManager` provides traditional buffered, persistent caching with **no async runtime dependency** — making it the recommended persistent backend for non-tokio clients such as smol-based `http-cache-ureq` or runtimes like Bevy. Now the **default** cache manager.

## [quick_cache](./quick-cache.md)

[`quick_cache`](https://github.com/arthurprs/quick-cache) is a lightweight and high performance concurrent cache optimized for low cache overhead. Provides traditional buffered caching operations.

## [streaming_cache](./streaming_cache.md)

[`StreamingManager`](https://github.com/06chaynes/http-cache/blob/main/http-cache/src/managers/streaming_cache.rs) uses an embedded [redb](https://github.com/cberner/redb) database for metadata and raw files on disk for response bodies, with [moka](https://github.com/moka-rs/moka) as an in-memory hot cache for metadata. Streams cached responses from disk in 64KB chunks without buffering entire bodies in memory. Ideal for large responses and long-lived caches that must survive restarts.
