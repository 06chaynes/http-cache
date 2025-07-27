//! Streaming HTTP caching example with large response bodies.
//!
//! This example demonstrates how to use the http-cache-tower middleware
//! with large response bodies to test caching performance and behavior.
//!
//! Run with: cargo run --example hyper_streaming --features manager-cacache

use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use http_cache::{CacheMode, HttpCache, HttpCacheOptions};
use http_cache_tower::{CACacheManager, HttpCacheLayer};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{SystemTime, UNIX_EPOCH};
use tower::{Service, ServiceBuilder};

// Generate large response content for testing streaming behavior
fn generate_large_content(size_kb: usize) -> String {
    let chunk =
        "This is a sample line of text for testing streaming cache behavior.\n";
    let lines_needed = (size_kb * 1024) / chunk.len();
    chunk.repeat(lines_needed)
}

/// A mock HTTP service that simulates different server responses with large payloads
/// This replaces the need for an actual HTTP server for the example
#[derive(Clone)]
struct LargeContentService;

impl Service<Request<Full<Bytes>>> for LargeContentService {
    type Response = Response<Full<Bytes>>;
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

    fn call(&mut self, req: Request<Full<Bytes>>) -> Self::Future {
        let path = req.uri().path().to_string();

        Box::pin(async move {
            // Simulate network delay
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            match path.as_str() {
                "/" => {
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/plain")
                        .header("cache-control", "max-age=60, public")
                        .body(Full::new(Bytes::from(format!(
                            "Large Content Cache Demo - Generated at: {timestamp}\n\nThis example tests caching with different payload sizes."
                        ))))?)
                }
                "/small" => {
                    let content = generate_large_content(1); // 1KB
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    println!(
                        "Generated small content ({} bytes)",
                        content.len()
                    );

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/plain")
                        .header("cache-control", "max-age=300, public") // Cache for 5 minutes
                        .header("x-content-size", &content.len().to_string())
                        .body(Full::new(Bytes::from(format!(
                            "Small Content (1KB) - Generated at: {}\n{}",
                            timestamp,
                            &content[..200.min(content.len())] // Truncate for readability
                        ))))?)
                }
                "/large" => {
                    let content = generate_large_content(1024); // 1MB
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    println!(
                        "Generated large content ({} bytes)",
                        content.len()
                    );

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/plain")
                        .header("cache-control", "max-age=600, public") // Cache for 10 minutes
                        .header("x-content-size", &content.len().to_string())
                        .body(Full::new(Bytes::from(format!(
                            "Large Content (1MB) - Generated at: {}\n{}",
                            timestamp,
                            &content[..500.min(content.len())] // Truncate for readability
                        ))))?)
                }
                "/huge" => {
                    let content = generate_large_content(5120); // 5MB
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    println!(
                        "Generated huge content ({} bytes)",
                        content.len()
                    );

                    // Simulate longer processing for huge content
                    tokio::time::sleep(std::time::Duration::from_millis(200))
                        .await;

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/plain")
                        .header("cache-control", "max-age=1800, public") // Cache for 30 minutes
                        .header("x-content-size", &content.len().to_string())
                        .header("x-streaming", "true")
                        .body(Full::new(Bytes::from(format!(
                            "Huge Content (5MB) - Generated at: {}\n{}",
                            timestamp,
                            &content[..1000.min(content.len())] // Truncate for readability
                        ))))?)
                }
                "/fresh" => {
                    let content = generate_large_content(512); // 512KB
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    println!(
                        "Generated fresh content ({} bytes)",
                        content.len()
                    );

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/plain")
                        .header("cache-control", "no-cache") // Always fresh
                        .header("x-content-size", &content.len().to_string())
                        .body(Full::new(Bytes::from(format!(
                            "Fresh Content (512KB) - Always Generated at: {}\n{}",
                            timestamp, &content[..300.min(content.len())] // Truncate for readability
                        ))))?)
                }
                "/api/data" => {
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    // Generate a large JSON response
                    let mut items = Vec::new();
                    for i in 0..1000 {
                        items.push(format!(
                            r#"{{"id": {i}, "name": "item_{i}", "description": "This is a sample item with some data", "timestamp": {timestamp}}}"#
                        ));
                    }
                    let json_data = format!(
                        r#"{{"message": "Large API response", "timestamp": {}, "items": [{}], "total": {}}}"#,
                        timestamp,
                        items.join(","),
                        items.len()
                    );

                    println!(
                        "Generated large JSON API response ({} bytes)",
                        json_data.len()
                    );

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .header("cache-control", "max-age=900, public") // Cache for 15 minutes
                        .body(Full::new(Bytes::from(json_data)))?)
                }
                "/slow" => {
                    let content = generate_large_content(256); // 256KB

                    // Simulate a slow endpoint with large content
                    tokio::time::sleep(std::time::Duration::from_millis(1000))
                        .await;

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/plain")
                        .header("cache-control", "max-age=120, public") // Cache for 2 minutes
                        .header("x-content-size", &content.len().to_string())
                        .body(Full::new(Bytes::from(format!(
                            "This was a slow response with large content!\n{}",
                            &content[..400.min(content.len())] // Truncate for readability
                        ))))?)
                }
                _ => Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header("content-type", "text/plain")
                    .body(Full::new(Bytes::from("Not Found\n")))?),
            }
        })
    }
}

async fn make_request<S, B, E>(
    service: &mut S,
    uri: &str,
    description: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: Service<Request<Full<Bytes>>, Response = Response<B>, Error = E>,
    E: std::fmt::Debug,
{
    let request = Request::builder().uri(uri).body(Full::new(Bytes::new()))?;

    println!("\n--- {description} ---");
    println!("Making request to: {uri}");

    let start = std::time::Instant::now();
    let response = service
        .call(request)
        .await
        .map_err(|e| format!("Service error: {e:?}"))?;
    let duration = start.elapsed();

    println!("Status: {}", response.status());
    println!("Response time: {duration:?}");

    // Print cache-related and content-size headers
    for (name, value) in response.headers() {
        let name_str = name.as_str();
        if name_str.starts_with("cache-")
            || name_str.starts_with("x-cache")
            || name_str.starts_with("x-content")
        {
            println!("Header {name}: {value:?}");
        }
    }

    println!("Response received successfully");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("HTTP Cache Tower Example - Large Content Testing");
    println!("================================================");

    // Create cache manager with disk storage
    let cache_dir = tempfile::tempdir()?;
    let cache_manager =
        CACacheManager::new(cache_dir.path().to_path_buf(), true);

    // Configure cache options
    let cache_options = HttpCacheOptions {
        cache_key: Some(Arc::new(|req: &http::request::Parts| {
            format!("{}:{}", req.method, req.uri)
        })),
        cache_status_headers: true, // Add X-Cache headers for debugging
        ..Default::default()
    };

    // Create HTTP cache with custom options
    let cache = HttpCache {
        mode: CacheMode::Default,
        manager: cache_manager,
        options: cache_options,
    };

    // Create the cache layer
    let cache_layer = HttpCacheLayer::with_cache(cache);

    // Build the service with caching middleware
    let mut service =
        ServiceBuilder::new().layer(cache_layer).service(LargeContentService);

    println!("Demonstrating HTTP caching with large response bodies...\n");

    // Scenario 1: Small content caching
    make_request(
        &mut service,
        "http://example.com/small",
        "Small content (1KB) - First request",
    )
    .await?;
    make_request(
        &mut service,
        "http://example.com/small",
        "Small content (1KB) - Second request (should be cached)",
    )
    .await?;

    // Scenario 2: Large content caching
    make_request(
        &mut service,
        "http://example.com/large",
        "Large content (1MB) - First request",
    )
    .await?;
    make_request(
        &mut service,
        "http://example.com/large",
        "Large content (1MB) - Second request (should be cached)",
    )
    .await?;

    // Scenario 3: Huge content caching (this will take longer to generate and cache)
    make_request(
        &mut service,
        "http://example.com/huge",
        "Huge content (5MB) - First request",
    )
    .await?;
    make_request(
        &mut service,
        "http://example.com/huge",
        "Huge content (5MB) - Second request (should be cached)",
    )
    .await?;

    // Scenario 4: Non-cacheable large content
    make_request(
        &mut service,
        "http://example.com/fresh",
        "Fresh content (512KB) - First request",
    )
    .await?;
    make_request(
        &mut service,
        "http://example.com/fresh",
        "Fresh content (512KB) - Second request (always fresh)",
    )
    .await?;

    // Scenario 5: Large JSON API response
    make_request(
        &mut service,
        "http://example.com/api/data",
        "Large JSON API - First request",
    )
    .await?;
    make_request(
        &mut service,
        "http://example.com/api/data",
        "Large JSON API - Second request (should be cached)",
    )
    .await?;

    // Scenario 6: Slow endpoint with large content
    make_request(
        &mut service,
        "http://example.com/slow",
        "Slow endpoint with large content (first request)",
    )
    .await?;
    make_request(
        &mut service,
        "http://example.com/slow",
        "Slow endpoint (cached - should be fast)",
    )
    .await?;

    Ok(())
}
