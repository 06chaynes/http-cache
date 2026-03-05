# Changelog

## [1.0.0-alpha.5] - 2026-02-17

### Changed

- Updated `http-cache` dependency to 1.0.0-alpha.5
- Updated `http-cache-semantics` to 3.0.0
- MSRV bumped from 1.85.0 to 1.88.0
- Exposed `url-standard`, `http-headers-compat`, `manager-cacache-bincode`, and `manager-moka-bincode` feature flags

## [1.0.0-alpha.4] - 2026-01-19

### Added

- `manager-foyer` feature flag for `FoyerManager` support
- `url-ada` feature flag for using WHATWG-compliant ada-url for URL parsing

### Changed

- Updated `http-cache` dependency to 1.0.0-alpha.4

## [1.0.0-alpha.3] - 2026-01-18

### Fixed

- Response body read failures now propagate errors instead of silently returning empty body

## [1.0.0-alpha.2] - 2026-01-18

### Added

- Response metadata integration for storing data with cached responses

### Changed

- MSRV is now 1.85.0

### Fixed

- Serialize all header values instead of just the first value per header name
- Read body to bytes correctly

## [1.0.0-alpha.1] - 2025-08-24

### Added

- Initial implementation of HTTP caching middleware for ureq