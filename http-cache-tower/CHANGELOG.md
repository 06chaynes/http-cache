# Changelog

## [1.0.0-alpha.4] - 2026-01-19

### Added

- `manager-foyer` feature flag for `FoyerManager` support
- `url-ada` feature flag for using WHATWG-compliant ada-url for URL parsing

### Fixed

- `OnlyIfCached` mode now correctly returns 504 Gateway Timeout on cache miss in streaming middleware

## [1.0.0-alpha.3] - 2026-01-18

### Added

- Response metadata integration for storing data with cached responses

### Changed

- MSRV is now 1.85.0

## [1.0.0-alpha.2] - 2025-08-24

### Added

- Support for cache-aware rate limiting through `rate_limiter` field in `HttpCacheOptions`
- New `rate-limiting` feature flag for optional rate limiting functionality
- Re-export of rate limiting types: `CacheAwareRateLimiter`, `DomainRateLimiter`, `DirectRateLimiter`, `Quota`
- Rate limiting integration for streaming cache operations via `HttpCacheStreamingLayer`
- `url` dependency (optional, enabled with rate-limiting feature) for URL parsing in rate limiting

### Changed

- Consolidated error handling: removed separate error module and replaced with type alias `pub use http_cache::HttpCacheError;`
- Simplified error architecture by removing duplicate error implementations
- Removed `anyhow` dependency, using manual error implementations throughout
- Fixed author field to include both authors for consistency with other crates

### Removed

- Dependency on `anyhow` for reduced dependency footprint

## [1.0.0-alpha.1] - 2025-07-27

### Added

- Initial release
