# Introduction

`http-cache` is a library that acts as a middleware for caching HTTP responses. It is intended to be used by other libraries to support multiple HTTP clients and backend cache managers, though it does come with multiple optional manager implementations out of the box. `http-cache` is built on top of [`http-cache-semantics`](https://github.com/kornelski/rusty-http-cache-semantics) which parses HTTP headers to correctly compute cacheability of responses.

## Key Features

- **Traditional Caching**: Standard HTTP response caching with full buffering
- **Streaming Support**: Memory-efficient caching for large responses without full buffering
- **Multiple Backends**: Support for disk-based (cacache) and in-memory (moka, quick-cache) storage
- **Client Integrations**: Support for reqwest, surf, and Tower/Hyper ecosystems
- **RFC 7234 Compliance**: Proper HTTP cache semantics with respect for cache-control headers

## Streaming vs Traditional Caching

The library supports two caching approaches:

- **Traditional Caching** (`CacheManager` trait): Buffers entire responses in memory before caching. Suitable for smaller responses and simpler use cases.
- **Streaming Caching** (`StreamingCacheManager` trait): Processes responses as streams without full buffering. Ideal for large files, media content, or memory-constrained environments.
