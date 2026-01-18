# Changelog

## [1.0.0-alpha.4] - 2026-01-18

### Added

- `empty_body` method to `StreamingCacheManager` trait for creating empty body responses
- `get_ref_count` method to `ContentRefCounter` for non-mutating reference count checks

### Changed

- `StreamingManager` now wraps `ContentRefCounter` in `Arc` to ensure all clones share the same state
- Atomic operations in streaming cache now use proper memory ordering (`Acquire`/`Release`/`AcqRel`) instead of `Relaxed`

### Fixed

- Race condition in `remove_ref` using atomic `compare_exchange` loop to prevent TOCTOU bugs
- Cache size and entry count divergence when `StreamingManager` is cloned
- Memory leak in `delete` where reference count was decremented but not restored on non-orphaned content
- Race condition in `delete` by using non-mutating `get_ref_count` instead of remove/add pattern

## [1.0.0-alpha.3] - 2026-01-18

### Added

- `modify_response` field to `HttpCacheOptions` for modifying responses before storing in cache
- `http-headers-compat` feature flag for header compatibility options
- `metadata` field to `HttpResponse` for storing arbitrary data with cached responses
- `metadata_provider` function to `HttpCacheOptions` for computing metadata on cache store

### Changed

- MSRV is now 1.85.0

### Fixed

- Serialize all header values instead of just the first value per header name
- `HttpHeaders` serialization and insert behavior for bincode compatibility
- Preserve all header values sharing the same name

## [1.0.0-alpha.2] - 2025-08-24

### Added

- `max_ttl` field to `HttpCacheOptions` for controlling maximum cache duration
- Support for `Duration` type in `max_ttl` field for better ergonomics and type safety
- Cache duration limiting functionality that overrides longer server-specified durations while respecting shorter ones
- Enhanced cache expiration control for `CacheMode::IgnoreRules` mode
- `rate_limiter` field to `HttpCacheOptions` for cache-aware rate limiting that only applies on cache misses
- `CacheAwareRateLimiter` trait for implementing rate limiting strategies
- `DomainRateLimiter` for per-domain rate limiting using governor
- `DirectRateLimiter` for global rate limiting using governor
- New `rate-limiting` feature flag for optional rate limiting functionality
- Rate limiting support for streaming cache operations with seamless integration
- Simple LRU eviction policy for the `StreamingManager` with configurable size and entry limits
- Multi-runtime async support (tokio/smol) with `RwLock` for better async performance
- Content deduplication using Blake3 hashing for efficient storage
- Atomic file operations using temporary files and rename for safe concurrent access
- Configurable streaming buffer size for optimal streaming performance
- Lock-free reference counting using DashMap for concurrent access
- LRU cache implementation using the `lru` crate

### Changed

- `max_ttl` implementation automatically enforces cache duration limits by modifying response cache-control headers
- Documentation updated with comprehensive examples for `max_ttl` usage across all cache modes
- `StreamingCacheConfig` simplified to essential configuration options:
  - `max_cache_size`: Optional cache size limit for LRU eviction
  - `max_entries`: Optional entry count limit for LRU eviction  
  - `streaming_buffer_size`: Buffer size for streaming operations (default: 8192)
- Enhanced error types and handling for streaming cache operations
- Simplified `StreamingManager` implementation focused on core functionality and maintainability
- Removed unused background cleanup and persistent reference counting infrastructure for cleaner codebase
- Improved async compatibility across tokio and smol runtimes
- Upgraded concurrent data structures to use DashMap and LRU cache
- Replaced custom implementations with established library solutions

### Fixed

- Race conditions in reference counting during concurrent access
- Resource leaks in streaming cache operations when metadata write fails
- Unsafe unwrap operations in cache entry manipulation
- Inefficient URL construction replaced with safer url crate methods
- Improved error handling and recovery in streaming operations

## [1.0.0-alpha.1] - 2025-07-27

### Added

- New streaming cache architecture for handling large HTTP responses without buffering entirely in memory
- `StreamingCacheManager` trait for streaming-aware cache backends
- `HttpCacheStreamInterface` trait for composable streaming middleware patterns
- `HttpStreamingCache` struct for managing streaming cache operations
- `StreamingManager` implementation using file-based storage
- `StreamingBody` type for handling both buffered and streaming scenarios
- `CacheAnalysis` struct for better separation of cache decision logic
- `response_cache_mode_fn` field to `HttpCacheOptions` for per-response cache mode overrides
- New streaming feature flags: `streaming`, `streaming-tokio`, `streaming-smol`

### Changed

- Refactored `Middleware` trait for better composability
- Cache manager interfaces now support both buffered and streaming operations
- Enhanced separation of concerns with discrete analysis/lookup/processing steps
- Renamed `cacache-async-std` feature to `cacache-smol` for consistency
- MSRV updated to 1.82.0

## [0.21.0] - 2025-06-25

### Added

- `remove_opts` field to `CACacheManager` struct. This field is an instance of `cacache::RemoveOpts` that allows for customization of the removal options when deleting items from the cache.

- MSRV is now 1.82.0

## [0.20.1] - 2025-01-30

### Changed

- Fixed missing implementation of CacheMode::Reload variant logic.

- MSRV is now 1.81.1

- Updated the minimum versions of the following dependencies:
  - async-trait [0.1.85]
  - cacache [13.1.0]
  - httpdate [1.0.2]
  - moka [0.12.10]
  - serde [1.0.217]
  - url [2.5.4]

## [0.20.0] - 2024-11-12

### Added

- `cache_status_headers` field to `HttpCacheOptions` struct. This field is a boolean that determines if the cache status headers should be added to the response.

## [0.19.0] - 2024-04-10

### Changed

- Updated the minimum versions of the following dependencies:
  - cacache [13.0.0]
  - http [1.1.0]
  - http-cache-semantics [2.1.0]

## [0.18.0] - 2024-01-15

### Added

- `overridden_cache_mode` method to `Middleware` trait. This method allows for overriding any cache mode set in the configuration, including `cache_mode_fn`.

- Derive `Default` for the `CacheMode` enum with the mode `Default` selected to be used.

## [0.17.0] - 2023-11-01

### Added

- `cache_mode_fn` field to `HttpCacheOptions` struct. This is a closure that takes a `&http::request::Parts` and returns a `CacheMode` enum variant. This allows for the overriding of cache mode on a per-request basis.

- `cache_bust` field to `HttpCacheOptions` struct. This is a closure that takes `http::request::Parts`, `Option<CacheKey>`, the default cache key (`&str`) and returns `Vec<String>` of keys to bust the cache for.

### Changed

- Updated the minimum versions of the following dependencies:
  - cacache [12.0.0]

## [0.16.0] - 2023-09-28

### Added

- `can_cache_request` method to `HttpCache` struct. This can be used by client implementations to determine if the request should be cached.

- `run_no_cache` method to `HttpCache` struct. This should be run by client implementations if the request is determined to not be cached.

### Changed

- MSRV is now 1.67.1

## [0.15.0] - 2023-09-26

### Added

- `IgnoreRules` variant to the `CacheMode` enum. This mode will ignore the HTTP headers and always store a response given it was a 200 response. It will also ignore the staleness when retrieving a response from the cache, so expiration of the cached response will need to be handled manually. If there was no cached response it will create a normal request, and will update the cache with the response.

### Changed

- Updated the minimum versions of the following dependencies:
  - moka [0.12.0]

## [0.14.0] - 2023-07-28

### Added

- `cacache-async-std` feature, which enables `async_std` runtime support in the `cacache` backend manager. This feature is enabled by default.

- `cacache-tokio` feature, which enables `tokio` runtime support in the `cacache` backend manager. This feature is disabled by default.

### Changed

- Updated the minimum versions of the following dependencies:
  - async-std [1.12.0]
  - async-trait [0.1.72]
  - serde [1.0.178]
  - tokio [1.29.1]

## [0.13.0] - 2023-07-19

### Added

- `CacheKey` type, a closure that takes [`http::request::Parts`] and returns a [`String`].

- `HttpCacheOptions` struct that contains the cache key (`CacheKey`) and the cache options (`CacheOptions`).

### Changed

- `CacheManager` trait `get`, `put`, and `delete` methods now require a `cache_key` argument rather than `method` and `url` arguments. This allows for custom keys to be specified.

- Both the `CACacheManager` trait and `MokaManager` implementation have been updated to reflect the above change.

- Updated the minimum versions of the following dependencies:
  - async-trait [0.1.71]
  - moka [0.11.2]
  - serde [1.0.171]

## [0.12.0] - 2023-06-05

### Changed

- MSRV is now 1.66.1
- `CACacheManager` field `path` has changed to `std::path::PathBuf`

- Updated the minimum versions of the following dependencies:
  - cacache [11.6.0]
  - moka [0.11.1]
  - serde [1.0.163]
  - url [2.4.0]

## [0.11.0] - 2023-03-29

### Added

- `BoxError` type alias for `Box<dyn std::error::Error + Send + Sync>`.

- `BadVersion` error type for unknown http versions.

- `BadHeader` error type for bad http header values.

### Removed

- `CacheError` enum.

- The following dependencies:
  - anyhow
  - thiserror
  - miette

### Changed

- `CacheError` enum has been replaced in function by `Box<dyn std::error::Error + Send + Sync>`.

- `Result` typedef is now `std::result::Result<T, BoxError>`.

- `Error` type for the TryFrom implentation for the `HttpVersion` struct is now `BoxError` containing a `BadVersion` error.

- `CacheManager` trait `put` method now returns `Result<(), BoxError>`.

- Updated the minimum versions of the following dependencies:
  - async-trait [0.1.68]
  - cacache [11.4.0]
  - moka [0.10.1]
  - serde [1.0.159]

## [0.10.1] - 2023-03-08

### Changed

- Set conditional check for `CacheError::Bincode` to `cfg(feature = "bincode")`

## [0.10.0] - 2023-03-08

### Changed

- MSRV is now 1.63.0

- Updated the minimum versions of the following dependencies:
  - async-trait [0.1.66]
  - cacache [11.3.0]
  - serde [1.0.154]
  - thiserror [1.0.39]

## [0.9.2] - 2023-02-23

### Changed

- Updated the minimum versions of the following dependencies:
  - cacache [11.1.0]

## [0.9.1] - 2023-02-17

### Changed

- Updated the minimum versions of the following dependencies:
  - http [0.2.9]

## [0.9.0] - 2023-02-16

### Changed

- MSRV is now 1.62.1

- Updated the minimum versions of the following dependencies:
  - moka [0.10.0]

## [0.8.0] - 2023-02-07

### Changed

- MSRV is now 1.60.0

- Updated the minimum versions of the following dependencies:
  - anyhow [1.0.69]
  - async-trait [0.1.64]
  - cacache [11.0.0]
  - miette [5.5.0]
  - moka [0.9.7]
  - serde [1.0.152]
  - thiserror [1.0.38]

## [0.7.2] - 2022-11-16

- Added derive `Eq` to `HttpVersion` enum.

### Changed

## [0.7.1] - 2022-11-06

### Changed

- Updated the minimum versions of the following dependencies:
  - anyhow [1.0.66]
  - async-trait [0.1.58]
  - miette [5.4.1]
  - moka [0.9.6]
  - serde [1.0.147]
  - thiserror [1.0.37]
  - url [2.3.1]

## [0.7.0] - 2022-06-17

### Changed

- The `CacheManager` trait is now implemented directly against the `MokaManager` struct rather than `Arc<MokaManager>`. The Arc is now internal to the `MokaManager` struct as part of the `cache` field.

- Updated the minimum versions of the following dependencies:
  - async-trait [0.1.56]
  - http [0.2.8]
  - miette [4.7.1]
  - moka [0.8.5]
  - serde [1.0.137]
  - thiserror [1.0.31]

## [0.6.5] - 2022-04-30

### Changed

- Updated the minimum versions of the following dependencies:
  - http [0.2.7]

## [0.6.4] - 2022-04-26

### Added

- This changelog to keep a record of notable changes to the project.
