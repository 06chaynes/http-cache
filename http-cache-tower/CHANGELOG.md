# Changelog

## [1.0.0-alpha.2] - 2025-08-20

### Changed

- Renamed `HttpCacheError` to `TowerError` for consistent `{CrateName}Error` naming convention
- Removed `anyhow` dependency, using manual error implementations throughout
- Fixed author field to include both authors for consistency with other crates

### Removed

- Dependency on `anyhow` for reduced dependency footprint

## [1.0.0-alpha.1] - 2025-07-27

### Added

- Initial release
