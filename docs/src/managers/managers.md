# Backend Cache Manager Implementations

The following backend cache manager implementations are provided by this crate:

## [cacache](./cacache.md)

[`cacache`](https://github.com/zkat/cacache-rs) is a high-performance, concurrent, content-addressable disk cache, optimized for async APIs. Provides traditional buffered caching.

## [moka](./moka.md)

[`moka`](https://github.com/moka-rs/moka) is a fast, concurrent cache library inspired by the Caffeine library for Java. Provides in-memory caching with traditional buffering.

## [foyer](./foyer.md)

[`foyer`](https://github.com/foyer-rs/foyer) is a hybrid in-memory + disk cache that provides configurable eviction strategies (w-TinyLFU, S3-FIFO, SIEVE), optional disk storage, request deduplication, and Tokio-native async operations.

## [quick_cache](./quick-cache.md)

[`quick_cache`](https://github.com/arthurprs/quick-cache) is a lightweight and high performance concurrent cache optimized for low cache overhead. Provides traditional buffered caching operations.

## [streaming_cache](./streaming_cache.md)

[`StreamingManager`](https://github.com/06chaynes/http-cache/blob/main/http-cache/src/managers/streaming_cache.rs) is a file-based streaming cache manager that does not buffer response bodies in memory. Suitable for handling large responses efficiently.
