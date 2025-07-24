# Backend Cache Manager Implementations

The following backend cache manager implementations are provided by this crate:

## [cacache](./cacache.md)

[`cacache`](https://github.com/zkat/cacache-rs) is a high-performance, concurrent, content-addressable disk cache, optimized for async APIs. Supports both traditional and streaming caching.

## [moka](./moka.md)

[`moka`](https://github.com/moka-rs/moka) is a fast, concurrent cache library inspired by the Caffeine library for Java. Provides in-memory caching with traditional buffering.

## [quick_cache](./quick_cache.md)

[`quick_cache`](https://github.com/arthurprs/quick-cache) is a lightweight and high performance concurrent cache optimized for low cache overhead. Supports both traditional and streaming caching operations.

## [streaming_cache](./streaming_cache.md)

[`FileCacheManager`](https://github.com/06chaynes/http-cache/blob/main/http-cache/src/managers/streaming_cache.rs) is a file-based streaming cache manager that does not buffer response bodies in memory. Suitable for handling large responses efficiently.
