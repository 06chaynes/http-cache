# Changelog

## [0.12.0] - 2023-11-01

### Added

- The following fields to `HttpCacheOptions` struct:
  - `cache_mode_fn` field. This is a closure that takes a `&http::request::Parts` and returns a `CacheMode` enum variant. This allows for the overriding of cache mode on a per-request basis.
  - `cache_bust` field. This is a closure that takes `http::request::Parts`, `Option<CacheKey>`, the default cache key (`&str`) and returns `Vec<String>` of keys to bust the cache for.

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.17.0]

## [0.11.3] - 2023-09-28

### Changed

- MSRV is now 1.67.1

- Implemented check to determine if a request is cacheable before running, avoiding the core logic if not.

- Updated the minimum versions of the following dependencies:
  - http-cache [0.16.0]

## [0.11.2] - 2023-09-26

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.15.0]

## [0.11.1] - 2023-07-28

### Changed

- Using new `cacache-tokio` feature in `http-cache` dependency

- Exporting `CacheManager` trait

- Updated the minimum versions of the following dependencies:
  - http-cache [0.14.0]
  - async-trait [0.1.72]
  - serde [1.0.178]

## [0.11.0] - 2023-07-19

### Changed

- Implemented new `HttpCacheOptions` struct

- Updated the minimum versions of the following dependencies:
  - http-cache [0.13.0]
  - anyhow [1.0.72]
  - async-trait [0.1.71]
  - serde [1.0.171]
  - tokio [1.29.1]

## [0.10.0] - 2022-06-05

### Changed

- MSRV is now 1.66.1
- Updated the minimum versions of the following dependencies:
  - http-cache [0.12.0]
  - anyhow [1.0.71]
  - reqwest [0.11.18]
  - reqwest-middleware [0.2.2]
  - serde [1.0.163]
  - tokio [1.28.2]
  - url [2.4.0]

## [0.9.0] - 2022-03-29

### Added

- `BadRequest` error type for request parsing failure.

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.11.0]
  - anyhow [1.0.70]
  - async-trait [0.1.68]
  - reqwest [0.11.16]
  - reqwest-middleware [0.2.1]
  - serde [1.0.159]
  - task-local-extensions [0.1.4]
  - tokio [1.27.0]

## [0.8.0] - 2023-03-08

### Changed

- MSRV is now 1.63.0
- Set `default-features = false` for `http-cache` dependency

- Updated the minimum versions of the following dependencies:
  - http-cache [0.10.1]
  - async-trait [0.1.66]
  - serde [1.0.154]
  - tokio [1.26.0]

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
  - reqwest [0.11.14]
  - serde [1.0.152]
  - tokio [1.25.0]

## [0.5.2] - 2022-11-16

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.7.2]
  - reqwest [0.11.13]
  - reqwest-middleware [0.2.0]

## [0.5.1] - 2022-11-06

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.7.1]
  - anyhow [1.0.66]
  - async-trait [0.1.58]
  - reqwest [0.11.12]
  - serde [1.0.147]
  - url [2.3.1]
  - task-local-extensions [0.1.3]
  - tokio [1.21.2]

## [0.5.0] - 2022-06-17

### Changed

- The `CacheManager` trait is now implemented directly against the `MokaManager` struct rather than `Arc<MokaManager>`. The Arc is now internal to the `MokaManager` struct as part of the `cache` field.

- Updated the minimum versions of the following dependencies:
  - http-cache [0.7.0]
  - async-trait [0.1.56]
  - http [0.2.8]
  - reqwest [0.11.11]
  - serde [1.0.137]
  - tokio [1.19.2]

## [0.4.5] - 2022-04-30

### Changed

- Updated the minimum versions of the following dependencies:
  - http-cache [0.6.5]
  - http [0.2.7]
  - tokio [1.18.0]

## [0.4.4] - 2022-04-26

### Added

- This changelog to keep a record of notable changes to the project.
