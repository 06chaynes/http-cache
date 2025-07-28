# Client Implementations

The following client implementations are provided by this crate:

## [reqwest](./reqwest.md)

The [`http-cache-reqwest`](https://github.com/06chaynes/http-cache/tree/main/http-cache-reqwest) crate provides a [`Middleware`](https://docs.rs/http-cache/latest/http_cache/trait.Middleware.html) implementation for the [`reqwest`](https://github.com/seanmonstar/reqwest) HTTP client.

## [surf](./surf.md)

The [`http-cache-surf`](https://github.com/06chaynes/http-cache/tree/main/http-cache-surf) crate provides a [`Middleware`](https://docs.rs/http-cache/latest/http_cache/trait.Middleware.html) implementation for the [`surf`](https://github.com/http-rs/surf) HTTP client.

## [tower](./tower.md)

The [`http-cache-tower`](https://github.com/06chaynes/http-cache/tree/main/http-cache-tower) crate provides Tower Layer and Service implementations for caching HTTP requests and responses. It supports both regular and streaming cache operations for memory-efficient handling of large responses.
