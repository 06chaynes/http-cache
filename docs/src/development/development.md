# Development

`http-cache` is meant to be extended to support multiple HTTP clients and backend cache managers. A [`CacheManager`](https://docs.rs/http-cache/latest/http_cache/trait.CacheManager.html) trait has been provided to help ease support for new backend cache managers. Similarly, a [`Middleware`](https://docs.rs/http-cache/latest/http_cache/trait.Middleware.html) trait has been provided to help ease supporting new HTTP clients.

## [Supporting a Backend Cache Manager](./supporting-a-backend-cache-manager.md)

This section is intended for those looking to implement a custom backend cache manager, or understand how the [`CacheManager`](https://docs.rs/http-cache/latest/http_cache/trait.CacheManager.html) trait works.

## [Supporting an HTTP Client](./supporting-an-http-client.md)

This section is intended for those looking to implement a custom HTTP client, or understand how the [`Middleware`](https://docs.rs/http-cache/latest/http_cache/trait.Middleware.html) trait works.
