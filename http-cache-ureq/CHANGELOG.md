# Changelog

## [1.0.0-alpha.2] - 2025-08-22

### Added

- Support for cache-aware rate limiting through `rate_limiter` field in `HttpCacheOptions`
- New `rate-limiting` feature flag for optional rate limiting functionality
- Re-export of rate limiting types: `CacheAwareRateLimiter`, `DomainRateLimiter`, `DirectRateLimiter`, `Quota`

### Changed

- Updated to use http-cache 1.0.0-alpha.2

## [1.0.0-alpha.1] - 2025-08-21

### Added

- Initial implementation of HTTP caching middleware for ureq