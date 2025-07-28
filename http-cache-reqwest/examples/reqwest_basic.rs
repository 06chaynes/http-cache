//! Basic HTTP caching example with reqwest client.
//!
//! This example demonstrates how to use the http-cache-reqwest middleware
//! with a reqwest client to cache HTTP responses automatically.
//!
//! Run with: cargo run --example reqwest_basic --features manager-cacache

use http_cache::{CacheMode, HttpCache, HttpCacheOptions};
use http_cache_reqwest::{CACacheManager, Cache};
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

async fn setup_mock_server() -> MockServer {
    let mock_server = MockServer::start().await;

    // Root endpoint - cacheable for 1 minute
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(|_: &wiremock::Request| {
            let timestamp =
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            ResponseTemplate::new(200)
                .set_body_string(format!(
                    "Hello from cached response! Generated at: {timestamp}\n"
                ))
                .append_header("content-type", "text/plain")
                .append_header("cache-control", "max-age=60, public")
        })
        .mount(&mock_server)
        .await;

    // Fresh endpoint - never cached
    Mock::given(method("GET"))
        .and(path("/fresh"))
        .respond_with(|_: &wiremock::Request| {
            let timestamp =
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            ResponseTemplate::new(200)
                .set_body_string(format!(
                    "Fresh response! Generated at: {timestamp}\n"
                ))
                .append_header("content-type", "text/plain")
                .append_header("cache-control", "no-cache")
        })
        .mount(&mock_server)
        .await;

    // API endpoint - cacheable for 5 minutes
    Mock::given(method("GET"))
        .and(path("/api/data"))
        .respond_with(|_: &wiremock::Request| {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            ResponseTemplate::new(200)
                .set_body_string(format!(
                    r#"{{"message": "API data", "timestamp": {timestamp}, "cached": true}}"#
                ))
                .append_header("content-type", "application/json")
                .append_header("cache-control", "max-age=300, public")
        })
        .mount(&mock_server)
        .await;

    // Slow endpoint - cacheable for 2 minutes
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(|_: &wiremock::Request| {
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_millis(1000))
                .set_body_string("This was a slow response!\n")
                .append_header("content-type", "text/plain")
                .append_header("cache-control", "max-age=120, public")
        })
        .mount(&mock_server)
        .await;

    mock_server
}

async fn make_request(
    client: &ClientWithMiddleware,
    url: &str,
    description: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\n--- {description} ---");
    println!("Making request to: {url}");

    let start = std::time::Instant::now();
    let response = client.get(url).send().await?;
    let duration = start.elapsed();

    println!("Status: {}", response.status());
    println!("Response time: {duration:?}");

    // Print cache-related headers
    for (name, value) in response.headers() {
        let name_str = name.as_str();
        if name_str.starts_with("cache-") || name_str.starts_with("x-cache") {
            if let Ok(value_str) = value.to_str() {
                println!("Header {name}: {value_str}");
            }
        }
    }

    let body = response.text().await?;
    println!("Response body: {}", body.trim());
    println!("Response received successfully");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("HTTP Cache Reqwest Example - Client Side");
    println!("=========================================");

    // Set up mock server
    let mock_server = setup_mock_server().await;
    let base_url = mock_server.uri();

    // Create cache manager with disk storage
    let cache_dir = tempdir()?;
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

    // Build the client with caching middleware
    let client = ClientBuilder::new(Client::new()).with(Cache(cache)).build();

    println!("Demonstrating HTTP caching with different scenarios...\n");

    // Scenario 1: Cacheable response
    make_request(
        &client,
        &format!("{base_url}/"),
        "First request to cacheable endpoint",
    )
    .await?;
    make_request(
        &client,
        &format!("{base_url}/"),
        "Second request (should be cached)",
    )
    .await?;

    // Scenario 2: Non-cacheable response
    make_request(
        &client,
        &format!("{base_url}/fresh"),
        "Request to no-cache endpoint",
    )
    .await?;
    make_request(
        &client,
        &format!("{base_url}/fresh"),
        "Second request to no-cache (always fresh)",
    )
    .await?;

    // Scenario 3: API endpoint with longer cache
    make_request(
        &client,
        &format!("{base_url}/api/data"),
        "API request (5min cache)",
    )
    .await?;
    make_request(
        &client,
        &format!("{base_url}/api/data"),
        "Second API request (should be cached)",
    )
    .await?;

    // Scenario 4: Slow endpoint
    make_request(
        &client,
        &format!("{base_url}/slow"),
        "Slow endpoint (first request)",
    )
    .await?;
    make_request(
        &client,
        &format!("{base_url}/slow"),
        "Slow endpoint (cached - should be fast)",
    )
    .await?;

    Ok(())
}
