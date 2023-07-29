
# Changelog

## [0.5.1] - 2023-07-28

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.14.0]
  - async-trait [0.1.72]
  - serde [1.0.178]
  - tokio [1.29.1]

## [0.5.0] - 2023-07-19

### Changed

- `CacheManager` trait `get`, `put`, and `delete` methods now require a `cache_key` argument rather than `method` and `url` arguments. This allows for custom keys to be specified.

- The `QuickManager` trait implementation has been updated to reflect the above change.

- Updated the minimum versions of the following dependencies:
  - http-cache [0.13.0]
  - async-trait [0.1.71]
  - serde [1.0.171]

## [0.4.0] - 2023-06-05

### Changed

- MSRV is now 1.66.1
- Updated the minimum versions of the following dependencies:
  - http-cache [0.12.0]
  - serde [1.0.163]
  - quick_cache [0.3.0]
  - url [2.4.0]

## [0.3.0] - 2023-03-29

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.11.0]
  - async-trait [0.1.68]
  - serde [1.0.159]
  - quick_cache [0.2.4]

## [0.2.0] - 2023-03-08

### Changed

- MSRV is now 1.63.0
- Set `default-features = false` for `http-cache` dependency

- Updated the minimum versions of the following dependencies:
  - http-cache [0.10.1]
  - async-trait [0.1.66]
  - serde [1.0.154]

## [0.1.2] - 2023-02-23

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.9.2]
  - quick_cache [0.2.2]

## [0.1.1] - 2023-02-17

### Changed

- Updated the minimum versions of the following dependencies:
  - quick_cache [0.2.1]

## [0.1.0] - 2023-02-16

### Added

- Initial release
