# Introduction

`http-cache` is a library that acts as a middleware for caching HTTP responses. It is intended to be used by other libraries to support multiple HTTP clients and backend cache managers, though it does come with two optional manager implementations out of the box. `http-cache` is built on top of [`http-cache-semantics`](https://github.com/kornelski/rusty-http-cache-semantics) which parses HTTP headers to correctly compute cacheability of responses.
