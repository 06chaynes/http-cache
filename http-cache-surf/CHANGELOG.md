# Changelog

## [0.14.2] - 2025-06-25

### Changed

- MSRV is now 1.82.0

- Updated the minimum versions of the following dependencies:
  - http-cache [0.21.0]

## [0.14.1] - 2025-01-30

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.20.1]
  - anyhow [1.0.95]
  - async-trait [0.1.85]
  - http [1.2.0]
  - serde [1.0.217]
  - url [2.5.4]
  - thiserror [2.0.11]

## [0.14.0] - 2024-11-12

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.20.0]
  - thiserror [2.0.3]

## [0.13.0] - 2024-04-10

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.19.0]
  - http-cache-semantics [2.1.0]
  - http [1.1.0]

## [0.12.1] - 2024-01-15

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.18.0]

## [0.12.0] - 2023-11-01

### Added

- The following fields to `HttpCacheOptions` struct:
- `cache_mode_fn` field. This is a closure that takes a `&http::request::Parts` and returns a `CacheMode` enum variant. This allows for the overriding of cache mode on a per-request basis.
- `cache_bust` field. This is a closure that takes `http::request::Parts`, `Option<CacheKey>`, the default cache key (`&str`) and returns `Vec<String>` of keys to bust the cache for.

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.17.0]

## [0.11.4] - 2023-09-28

### Changed

- MSRV is now 1.67.1

- Implemented check to determine if a request is cacheable before running, avoiding the core logic if not.

- Updated the minimum versions of the following dependencies:
  - http-cache [0.16.0]

## [0.11.3] - 2023-09-26

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.15.0]

## [0.11.2] - 2023-07-28

### Changed

- Using new `cacache-async-std` feature in `http-cache` dependency

- Exporting `CacheManager` trait

- Updated the minimum versions of the following dependencies:
  - http-cache [0.14.0]
  - async-trait [0.1.72]
  - serde [1.0.178]
  - thiserror [1.0.44]

## [0.11.1] - 2023-07-22

### Changed

- Set `default-features` to `false` for `surf` dependency.

## [0.11.0] - 2023-07-19

### Changed

- Implemented new `HttpCacheOptions` struct

- Updated the minimum versions of the following dependencies:
  - http-cache [0.13.0]
  - anyhow [1.0.72]
  - async-trait [0.1.71]
  - serde [1.0.171]
  - thiserror [1.0.43]

## [0.10.0] - 2022-06-05

### Changed

- MSRV is now 1.66.1
- Updated the minimum versions of the following dependencies:
  - http-cache [0.12.0]
  - anyhow [1.0.71]
  - serde [1.0.163]
  - url [2.4.0]

## [0.9.0] - 2022-03-29

### Added

- A generic error type `Error` deriving thiserror::Error

- The following dependencies:
  - thiserror

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.11.0]
  - anyhow [1.0.70]
  - async-trait [0.1.68]
  - serde [1.0.159]
  - thiserror [1.0.40]

## [0.8.0] - 2023-03-08

### Changed

- MSRV is now 1.63.0
- Set `default-features = false` for `http-cache` dependency

- Updated the minimum versions of the following dependencies:
  - http-cache [0.10.1]
  - async-trait [0.1.66]
  - serde [1.0.154]

## [0.7.2] - 2023-02-23

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.9.2]

## [0.7.1] - 2023-02-17

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.9.1]
  - http [0.2.9]

## [0.7.0] - 2023-02-16

### Changed

- MSRV is now 1.62.1

- Updated the minimum versions of the following dependencies:
  - http-cache [0.9.0]

## [0.6.0] - 2023-02-07

- MSRV is now 1.60.0

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.8.0]
  - anyhow [1.0.69]
  - async-trait [0.1.64]
  - serde [1.0.152]

## [0.5.2] - 2022-11-16

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.7.2]

## [0.5.1] - 2022-11-06

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.7.1]
  - anyhow [1.0.66]
  - async-trait [0.1.58]
  - serde [1.0.147]
  - url [2.3.1]
  - async-std [1.12.0]

## [0.5.0] - 2022-06-17

### Changed

- The `CacheManager` trait is now implemented directly against the `MokaManager` struct rather than `Arc<MokaManager>`. The Arc is now internal to the `MokaManager` struct as part of the `cache` field.

- Updated the minimum versions of the following dependencies:
  - http-cache [0.7.0]
  - async-trait [0.1.56]
  - http [0.2.8]
  - serde [1.0.137]

## [0.4.6] - 2022-04-30

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.6.5]
  - http [0.2.7]

## [0.4.5] - 2022-04-26

### Fixed

- Updated version of http-cache to 0.6.4. I apparently have forgotten to do this the last couple of updates!

## [0.4.4] - 2022-04-26

### Added

- This changelog to keep a record of notable changes to the project.
