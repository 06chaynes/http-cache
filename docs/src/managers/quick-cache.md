# quick_cache

[`quick_cache`](https://github.com/arthurprs/quick-cache) is a lightweight and high performance concurrent cache optimized for low cache overhead. The `http-cache-quickcache` implementation provides both traditional and streaming caching capabilities.

## Getting Started

The `quick_cache` backend cache manager is provided by the [`http-cache-quickcache`](https://github.com/06chaynes/http-cache/tree/main/http-cache-quickcache) crate.

```sh
cargo add http-cache-quickcache
```

## Basic Usage with Tower

The quickcache manager works excellently with Tower services:

```rust
use tower::{Service, ServiceExt};
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use http_cache_quickcache::QuickManager;
use std::convert::Infallible;

// Example Tower service that uses QuickManager for caching
#[derive(Clone)]
struct CachingService {
    cache_manager: QuickManager,
}

impl Service<Request<Full<Bytes>>> for CachingService {
    type Response = Response<Full<Bytes>>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Full<Bytes>>) -> Self::Future {
        let manager = self.cache_manager.clone();
        Box::pin(async move {
            // Cache logic using the manager would go here
            let response = Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::from("Hello from cached service!")))?;
            Ok(response)
        })
    }
}
```

## Streaming Support

For large responses, use the streaming capabilities:

```rust
use http_cache_quickcache::QuickManager;
use http_cache::StreamingCacheManager;
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use http_cache_semantics::CachePolicy;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = QuickManager::default();
    
    // Create a sample response for caching
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("cache-control", "max-age=3600")
        .body(Full::new(Bytes::from("Hello, streaming world!")))?;
    
    // Create request for policy
    let request = Request::builder()
        .method("GET")
        .uri("https://example.com/data")
        .body(())?;
    
    let policy = CachePolicy::new(&request, &Response::builder()
        .status(200)
        .body(b"Hello, streaming world!".to_vec())?);
    
    // Cache the streaming response
    let url = Url::parse("https://example.com/data")?;
    let cached = StreamingCacheManager::put(
        &manager,
        "GET:https://example.com/data".to_string(),
        response,
        policy,
        url,
    ).await?;
    
    println!("Cached response status: {}", cached.status());
    
    // Retrieve from cache
    let retrieved = StreamingCacheManager::get(&manager, "GET:https://example.com/data").await?;
    if let Some((response, _policy)) = retrieved {
        println!("Retrieved cached response with status: {}", response.status());
    }
    
    Ok(())
}
```

## Working with the manager directly

First construct your manager instance. This example will use the default cache configuration.

```rust
let manager = Arc::new(QuickManager::default());
```

You can also specify other configuration options. This uses the `new` methods on both `QuickManager` and `quick_cache::sync::Cache` to construct a cache with a maximum capacity of 100 items.

```rust
let manager = Arc::new(QuickManager::new(quick_cache::sync::Cache::new(100)));
```

### Traditional Cache Operations

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

### Streaming Cache Operations  

The QuickManager also supports streaming operations through the `StreamingCacheManager` trait, allowing for memory-efficient handling of large responses without full buffering.
