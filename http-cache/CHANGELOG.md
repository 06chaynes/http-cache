# Changelog

## [0.12.0] - 2023-06-05

### Changed

- MSRV is now 1.65.0
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
