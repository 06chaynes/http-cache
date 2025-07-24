# moka

[`moka`](https://github.com/moka-rs/moka) is a fast, concurrent cache library inspired by the Caffeine library for Java. The moka manager supports traditional caching operations and can be used with streaming cache wrappers for memory-efficient handling of large responses.

## Getting Started

The `moka` backend cache manager is provided by the `http-cache` crate but is not enabled by default. The `http-cache-reqwest`, `http-cache-surf`, and `http-cache-tower` crates all expose the types so no need to pull in the `http-cache` directly unless you need to implement your own client.

### reqwest

```sh
cargo add http-cache-reqwest --no-default-features -F manager-moka
```

### surf

```sh
cargo add http-cache-surf --no-default-features -F manager-moka
```

### tower

```sh
cargo add http-cache-tower --no-default-features -F manager-moka
```

## Working with the manager directly

First construct your manager instance. This example will use the default cache configuration (42).

```rust
let manager = Arc::new(MokaManager::default());
```

You can also specify other configuration options. This uses the `new` methods on both `MokaManager` and `moka::future::Cache` to construct a cache with a maximum capacity of 100 items.

```rust
let manager = Arc::new(MokaManager::new(moka::future::Cache::new(100)));
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

You can also clear the entire cache using the `clear` method. This method accepts no arguments and returns an `Result<(), BoxError>`.

```rust
manager.clear().await?;
```
