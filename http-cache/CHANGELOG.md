# Changelog

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
