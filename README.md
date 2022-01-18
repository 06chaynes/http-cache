# http-cache

[![CI](https://img.shields.io/github/workflow/status/06chaynes/http-cache/Rust?label=CI&style=for-the-badge)](https://github.com/06chaynes/http-cache/actions/workflows/rust.yml)
[![Crates.io](https://img.shields.io/crates/v/http-cache?style=for-the-badge)](https://crates.io/crates/http-cache)
[![Docs.rs](https://img.shields.io/docsrs/http-cache?style=for-the-badge)](https://docs.rs/http-cache)
![Codecov](https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge)
![Crates.io](https://img.shields.io/crates/l/http-cache?style=for-the-badge)

<img align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/latest/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">

A caching middleware that follows HTTP caching rules,
thanks to [http-cache-semantics](https://github.com/kornelski/rusty-http-cache-semantics).
By default, it uses [cacache](https://github.com/zkat/cacache-rs) as the backend cache manager.

## Install

With [cargo add](https://github.com/killercup/cargo-edit#Installation) installed :

```sh
cargo add http-cache
```

## Features

The following features are available. By default `manager-cacache` is enabled.

- `manager-cacache` (default): use [cacache](https://github.com/zkat/cacache-rs), a high-performance disk cache, for the manager backend.
- `with-http-types` (disabled): enable [http-types](https://github.com/http-rs/http-types) type conversion support

## Documentation

- [API Docs](https://docs.rs/http-cache)

## Provided Client Implementations
- **Surf**: See [README](http-cache-surf/README.md) for more details
- **Reqwest**: See [README](http-cache-reqwest/README.md) for more details

## License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
