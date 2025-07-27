//! Basic HTTP caching example with Hyper client and Tower middleware.
//!
//! This example demonstrates how to use the http-cache-tower middleware
//! with a Hyper client to cache HTTP responses automatically.
//!
//! Run with: cargo run --example hyper_basic --features manager-cacache

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

/// A mock HTTP service that simulates different server responses
/// This replaces the need for an actual HTTP server for the example
#[derive(Clone)]
struct MockHttpService;

impl Service<Request<Full<Bytes>>> for MockHttpService {
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
                        .header("cache-control", "max-age=60, public") // Cache for 1 minute
                        .body(Full::new(Bytes::from(format!(
                            "Hello from cached response! Generated at: {timestamp}\n"
                        ))))?)
                }
                "/fresh" => {
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/plain")
                        .header("cache-control", "no-cache") // Always fresh
                        .body(Full::new(Bytes::from(format!(
                            "Fresh response! Generated at: {timestamp}\n"
                        ))))?)
                }
                "/api/data" => {
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    // Simulate API response with JSON
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .header("cache-control", "max-age=300, public") // Cache for 5 minutes
                        .body(Full::new(Bytes::from(format!(
                            r#"{{"message": "API data", "timestamp": {timestamp}, "cached": true}}"#
                        ))))?)
                }
                "/slow" => {
                    // Simulate a slow endpoint
                    tokio::time::sleep(std::time::Duration::from_millis(1000))
                        .await;

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/plain")
                        .header("cache-control", "max-age=120, public") // Cache for 2 minutes
                        .body(Full::new(Bytes::from(
                            "This was a slow response!\n",
                        )))?)
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

    // Print cache-related headers
    for (name, value) in response.headers() {
        let name_str = name.as_str();
        if name_str.starts_with("cache-") || name_str.starts_with("x-cache") {
            println!("Header {name}: {value:?}");
        }
    }

    println!("Response received successfully");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("HTTP Cache Tower Example - Client Side");
    println!("======================================");

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
        ServiceBuilder::new().layer(cache_layer).service(MockHttpService);

    println!("Demonstrating HTTP caching with different scenarios...\n");

    // Scenario 1: Cacheable response
    make_request(
        &mut service,
        "http://example.com/",
        "First request to cacheable endpoint",
    )
    .await?;
    make_request(
        &mut service,
        "http://example.com/",
        "Second request (should be cached)",
    )
    .await?;

    // Scenario 2: Non-cacheable response
    make_request(
        &mut service,
        "http://example.com/fresh",
        "Request to no-cache endpoint",
    )
    .await?;
    make_request(
        &mut service,
        "http://example.com/fresh",
        "Second request to no-cache (always fresh)",
    )
    .await?;

    // Scenario 3: API endpoint with longer cache
    make_request(
        &mut service,
        "http://example.com/api/data",
        "API request (5min cache)",
    )
    .await?;
    make_request(
        &mut service,
        "http://example.com/api/data",
        "Second API request (should be cached)",
    )
    .await?;

    // Scenario 4: Slow endpoint
    make_request(
        &mut service,
        "http://example.com/slow",
        "Slow endpoint (first request)",
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
