//! Streaming HTTP caching with tower/hyper
//!
//! Run with: cargo run --example hyper_streaming --features streaming

#![cfg(feature = "streaming")]

use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::Full;
use http_cache::StreamingManager;
use http_cache_tower::HttpCacheStreamingLayer;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;
use tower::{Service, ServiceBuilder};

/// Mock service that simulates streaming content
#[derive(Clone)]
struct StreamingMockService {
    request_count: Arc<AtomicU32>,
}

impl StreamingMockService {
    fn new() -> Self {
        Self { request_count: Arc::new(AtomicU32::new(0)) }
    }
}

impl Service<Request<Full<Bytes>>> for StreamingMockService {
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
        let count = self.request_count.fetch_add(1, Ordering::SeqCst);

        Box::pin(async move {
            // Simulate network delay and large content generation
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // Generate large content (simulate streaming)
            let large_content = "X".repeat(50000); // 50KB of data
            let response_body = format!(
                "Streaming response #{}\nContent size: {} bytes\n{}",
                count + 1,
                large_content.len(),
                large_content
            );

            Ok(http::Response::builder()
                .status(StatusCode::OK)
                .header("cache-control", "max-age=300, public")
                .header("content-type", "text/plain")
                .header("x-content-size", response_body.len().to_string())
                .body(Full::new(Bytes::from(response_body)))?)
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cache_dir = tempfile::tempdir().unwrap();
    let streaming_manager =
        StreamingManager::new(cache_dir.path().to_path_buf());

    // Create the streaming cache layer
    let streaming_layer = HttpCacheStreamingLayer::new(streaming_manager);

    // Build the service with streaming cache middleware
    let mut service = ServiceBuilder::new()
        .layer(streaming_layer)
        .service(StreamingMockService::new());

    println!("Testing streaming HTTP caching with tower/hyper...");

    // First request - content will be cached as stream
    let start = Instant::now();
    let req = Request::builder()
        .uri("http://example.com/large-content")
        .body(Full::new(Bytes::new()))?;
    let response = service.call(req).await?;
    let duration1 = start.elapsed();

    println!("First request: {:?}", duration1);
    println!("Status: {}", response.status());

    let body1 = http_body_util::BodyExt::collect(response.into_body())
        .await?
        .to_bytes();
    println!("First response size: {} bytes", body1.len());

    // Second request - should be served from streaming cache (much faster)
    let start = Instant::now();
    let req = Request::builder()
        .uri("http://example.com/large-content")
        .body(Full::new(Bytes::new()))?;
    let response = service.call(req).await?;
    let duration2 = start.elapsed();

    println!("Second request: {:?}", duration2);
    println!("Status: {}", response.status());

    // Check cache headers before consuming the body
    for (name, value) in response.headers() {
        let name_str = name.as_str();
        if name_str.starts_with("x-cache") || name_str == "cache-control" {
            println!("Header {}: {:?}", name, value);
        }
    }

    let body2 = http_body_util::BodyExt::collect(response.into_body())
        .await?
        .to_bytes();
    println!("Second response size: {} bytes", body2.len());

    // Verify content consistency
    if body1.len() != body2.len() {
        println!("Warning: Content size mismatch");
    }

    Ok(())
}
