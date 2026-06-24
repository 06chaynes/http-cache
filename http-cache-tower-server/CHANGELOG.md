# Changelog

## [0.2.4] - 2026-05-29

### Added

- `manager-redb` feature flag for `RedbManager` support

### Changed

- Updated `http-cache` dependency to 1.0.0-alpha.7
- Default feature changed from `manager-cacache` to `manager-redb`; enable `manager-cacache` explicitly to keep using `CACacheManager`

### Fixed

- `tokio` dependency now enables the `rt` feature required for `tokio::spawn` (previously satisfied only transitively via the `manager-cacache` feature)

## [0.2.3] - 2026-04-17

### Changed

- Updated `http-cache` dependency to 1.0.0-alpha.6
- Removed `async-trait` dependency
- `CachedResponse.headers` now stores `HashMap<String, Vec<String>>` instead of `HashMap<String, String>` to preserve multi-valued headers (e.g. `Set-Cookie`). **Breaking serialization change**: cache entries written by 0.2.2 and earlier cannot be deserialized by 0.2.3; they fall back to cache miss on read (no data corruption) and will be re-populated on the next upstream request. Operators with persistent caches should expect a one-time warm-up after upgrade.
- `CachedResponse::into_response` now returns `Result<Response<Bytes>, BoxError>` instead of panicking on malformed header state.

## [0.2.2] - 2026-02-17

### Changed

- Updated `http-cache` dependency to 1.0.0-alpha.5
- Updated `http-cache-semantics` to 3.0.0
- MSRV bumped from 1.85.0 to 1.88.0
- Exposed `url-standard`, `url-ada`, `http-headers-compat`, `streaming`, `rate-limiting`, `manager-cacache-bincode`, and `manager-moka-bincode` feature flags

## [0.2.1] - 2026-01-18

### Changed

- Updated `http-cache` dependency to 1.0.0-alpha.4

## [0.2.0] - 2026-01-18

### Changed

- Updated `http-cache` dependency to 1.0.0-alpha.3
- MSRV is now 1.85.0

## [0.1.0] - 2025-11-23

### Added

- Initial release of server-side HTTP response caching middleware for Tower/Axum
