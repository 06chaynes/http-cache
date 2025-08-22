//! Basic HTTP caching with tower/hyper
//!
//! Run with: cargo run --example hyper_basic --features manager-cacache

use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::Full;
use http_cache::{CacheMode, HttpCache, HttpCacheOptions};
use http_cache_tower::{CACacheManager, HttpCacheLayer};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use tower::{Service, ServiceBuilder};

/// Simple mock service that simulates HTTP responses
#[derive(Clone)]
struct MockService {
    request_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

impl MockService {
    fn new() -> Self {
        Self {
            request_count: std::sync::Arc::new(
                std::sync::atomic::AtomicU32::new(0),
            ),
        }
    }
}

impl Service<Request<Full<Bytes>>> for MockService {
    type Response = http::Response<Full<Bytes>>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<
        Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request<Full<Bytes>>) -> Self::Future {
        let count = self
            .request_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        Box::pin(async move {
            // Simulate network delay for first request
            if count == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }

            let response_body =
                format!("Response #{} with caching enabled", count + 1);

            Ok(http::Response::builder()
                .status(StatusCode::OK)
                .header("cache-control", "max-age=300, public")
                .header("content-type", "text/plain")
                .body(Full::new(Bytes::from(response_body)))?)
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache_manager =
        CACacheManager::new(cache_dir.path().to_path_buf(), true);

    // Create HTTP cache
    let cache = HttpCache {
        mode: CacheMode::Default,
        manager: cache_manager,
        options: HttpCacheOptions {
            cache_status_headers: true,
            ..Default::default()
        },
    };

    // Create the cache layer
    let cache_layer = HttpCacheLayer::with_cache(cache);

    // Build the service with caching middleware
    let mut service =
        ServiceBuilder::new().layer(cache_layer).service(MockService::new());

    println!("Testing HTTP caching with tower/hyper...");

    // First request
    let start = Instant::now();
    let req = Request::builder()
        .uri("http://example.com/test")
        .body(Full::new(Bytes::new()))?;
    let response = service.call(req).await?;
    let duration1 = start.elapsed();

    println!("First request: {:?}", duration1);
    println!("Status: {}", response.status().as_u16());

    // Check cache headers after first request
    for (name, value) in response.headers() {
        let name_str = name.as_str();
        if name_str.starts_with("x-cache") {
            println!("Cache header {}: {}", name, value.to_str().unwrap_or(""));
        }
    }

    println!();

    // Second request (should be much faster due to caching)
    let start = Instant::now();
    let req = Request::builder()
        .uri("http://example.com/test")
        .body(Full::new(Bytes::new()))?;
    let response = service.call(req).await?;
    let duration2 = start.elapsed();

    println!("Second request: {:?}", duration2);
    println!("Status: {}", response.status().as_u16());

    // Check cache headers after second request
    for (name, value) in response.headers() {
        let name_str = name.as_str();
        if name_str.starts_with("x-cache") {
            println!("Cache header {}: {}", name, value.to_str().unwrap_or(""));
        }
    }

    Ok(())
}
