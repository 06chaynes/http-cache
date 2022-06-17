# Changelog

## [0.5.0] - 2022-06-17

### Changed

- The `CacheManager` trait is now implemented directly against the `MokaManager` struct rather than `Arc<MokaManager>`. The Arc is now internal to the `MokaManager` struct as part of the `cache` field.

- Updated the minimum versions of the following dependencies:
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
