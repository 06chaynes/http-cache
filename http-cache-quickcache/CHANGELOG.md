# Changelog

## [1.0.0-alpha.3] - 2026-01-18

### Changed

- MSRV is now 1.85.0

## [1.0.0-alpha.2] - 2025-08-24

### Changed

- Updated to use http-cache 1.0.0-alpha.2 with rate limiting support

## [1.0.0-alpha.1] - 2025-07-27

### Added

- Integration with updated core library traits for better composability

### Changed

- Updated to use http-cache 1.0.0-alpha.1
- MSRV updated to 1.82.0
- Made `cache` field private in `QuickManager`

## [0.9.0] - 2025-06-25

### Added

- `remove_opts` field to `CACacheManager` struct. This field is an instance of `cacache::RemoveOpts` that allows for customization of the removal options when deleting items from the cache.

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.21.0]

## [0.8.1] - 2025-01-30

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.20.1]
  - async-trait [0.1.85]
  - darkbird [6.2.4]
  - serde [1.0.217]
  - url [2.5.4]

## [0.8.0] - 2024-11-12

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.20.0]
  - quick_cache [0.6.9]

## [0.7.0] - 2024-04-10

### Changed

- MSRV is now 1.71.1

- Updated the minimum versions of the following dependencies:
  - http-cache [0.19.0]
  - http-cache-semantics [2.1.0]
  - http [1.1.0]
  - reqwest [0.12.3]
  - reqwest-middleware [0.3.0]
  - quick_cache [0.5.1]

## [0.6.3] - 2024-01-15

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.18.0]

## [0.6.2] - 2023-11-01

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.17.0]

## [0.6.1] - 2023-09-28

### Changed

- MSRV is now 1.67.1

- Updated the minimum versions of the following dependencies:
  - http-cache [0.16.0]

## [0.6.0] - 2023-09-26

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.15.0]
  - quick_cache [0.4.0]

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
