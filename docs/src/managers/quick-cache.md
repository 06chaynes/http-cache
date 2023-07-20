# quick_cache

[`quick_cache`](https://github.com/arthurprs/quick-cache) is a lightweight and high performance concurrent cache optimized for low cache overhead.

## Getting Started

The `quick_cache` backend cache manager is provided by the [`http-cache-quickcache`](https://github.com/06chaynes/http-cache/tree/latest/http-cache-quickcache) crate.

```sh
cargo add http-cache-quickcache
```

## Working with the manager directly

First construct your manager instance. This example will use the default cache configuration (42).

```rust
let manager = Arc::new(QuickManager::default());
```

You can also specify other configuration options. This uses the `new` methods on both `QuickManager` and `quick_cache::sync::Cache` to construct a cache with a maximum capacity of 100 items.

```rust
let manager = Arc::new(QuickManager::new(quick_cache::sync::Cache::new(100)));
```

You can attempt to retrieve a record from the cache using the `get` method. This method accepts a `&str` as the cache key and returns an `Result<Option<(HttpResponse, CachePolicy)>, BoxError>`.

```rust
let response = manager.get("my-cache-key").await?;
```

You can store a record in the cache using the `put` method. This method accepts a `String` as the cache key, a `HttpResponse` as the response, and a `CachePolicy` as the policy object. It returns an `Result<HttpResponse, BoxError>`. The below example constructs the response and policy manually, normally this would be handled by the middleware.

```rust
let url = Url::parse("http://example.com")?;
let response = HttpResponse {
    body: TEST_BODY.to_vec(),
    headers: Default::default(),
    status: 200,
    url: url.clone(),
    version: HttpVersion::Http11,
};
let req = http::Request::get("http://example.com").body(())?;
let res = http::Response::builder()
    .status(200)
    .body(TEST_BODY.to_vec())?;
let policy = CachePolicy::new(&req, &res);
let response = manager.put("my-cache-key".into(), response, policy).await?;
```

You can remove a record from the cache using the `delete` method. This method accepts a `&str` as the cache key and returns an `Result<(), BoxError>`.

```rust
manager.delete("my-cache-key").await?;
```
