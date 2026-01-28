# http-cache

[![CI](https://img.shields.io/github/actions/workflow/status/06chaynes/http-cache/http-cache.yml?label=CI&style=for-the-badge)](https://github.com/06chaynes/http-cache/actions/workflows/http-cache.yml)
[![Crates.io](https://img.shields.io/crates/v/http-cache?style=for-the-badge)](https://crates.io/crates/http-cache)
[![Docs.rs](https://img.shields.io/docsrs/http-cache?style=for-the-badge)](https://docs.rs/http-cache)
[![Codecov](https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge)](https://app.codecov.io/gh/06chaynes/http-cache)
![Crates.io](https://img.shields.io/crates/l/http-cache?style=for-the-badge)

<img class="logo" align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/main/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">

A caching middleware that follows HTTP caching rules,
thanks to [http-cache-semantics](https://github.com/kornelski/rusty-http-cache-semantics).
By default, it uses [cacache](https://github.com/zkat/cacache-rs) as the backend cache manager.

## How do I use this?

Likely you won't! At least not directly. Unless you are looking to implement a custom backend cache manager
or client middleware you'll probably want to pull in one of the existing client implementations instead.
See the [Provided Client Implementations](#provided-client-implementations) section below.

## Minimum Supported Rust Version (MSRV)

1.85.0

## Install

With [cargo add](https://github.com/killercup/cargo-edit#Installation) installed :

```sh
cargo add http-cache
```

## Features

The following features are available. By default `manager-cacache` and `url-standard` are enabled.

- `manager-cacache` (default): enable [cacache](https://github.com/zkat/cacache-rs), a high-performance disk cache, backend manager.
- `manager-moka` (disabled): enable [moka](https://github.com/moka-rs/moka), a high-performance in-memory cache, backend manager.
- `manager-foyer` (disabled): enable [foyer](https://github.com/foyer-rs/foyer), a hybrid in-memory + disk cache, backend manager.
- `streaming` (disabled): enable streaming cache support using [cacache](https://github.com/zkat/cacache-rs) for disk storage with [moka](https://github.com/moka-rs/moka) for metadata tracking and TinyLFU eviction.
- `rate-limiting` (disabled): enable rate limiting functionality with [governor](https://github.com/boinkor-net/governor).
- `url-standard` (default): enable [url](https://github.com/servo/rust-url) for URL parsing.
- `url-ada` (disabled): enable [ada-url](https://github.com/ada-url/rust) for WHATWG-compliant URL parsing (avoids Unicode/IDNA license).
- `with-http-types` (disabled): enable [http-types](https://github.com/http-rs/http-types) type conversion support

### URL Feature Notes

The `url-standard` and `url-ada` features are **mutually exclusive** - exactly one must be enabled.
The `url-standard` feature uses the `url` crate which depends on `idna` (Unicode license).
If you need to avoid the Unicode license, use `url-ada` instead:

```toml
[dependencies]
http-cache = { version = "1.0", default-features = false, features = ["manager-cacache", "url-ada"] }
```

**Breaking change for `default-features = false` users**: You must now explicitly enable either `url-standard` or `url-ada`.

## Documentation

- [API Docs](https://docs.rs/http-cache)

## Provided Client Implementations

- **Reqwest**: See [README](https://github.com/06chaynes/http-cache/blob/main/http-cache-reqwest/README.md) for more details
- **Tower**: See [README](https://github.com/06chaynes/http-cache/blob/main/http-cache-tower/README.md) for more details
- **Surf**: See [README](https://github.com/06chaynes/http-cache/blob/main/http-cache-surf/README.md) for more details
- **Ureq**: See [README](https://github.com/06chaynes/http-cache/blob/main/http-cache-ureq/README.md) for more details

## Server-Side Caching Middleware

- **Tower Server**: See [README](https://github.com/06chaynes/http-cache/blob/main/http-cache-tower-server/README.md) for more details

## Additional Manager Implementations

- **quick-cache**: See [README](https://github.com/06chaynes/http-cache/blob/main/http-cache-quickcache/README.md) for more details

## License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](https://github.com/06chaynes/http-cache/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license
  ([LICENSE-MIT](https://github.com/06chaynes/http-cache/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
