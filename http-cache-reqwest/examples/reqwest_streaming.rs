//! Streaming HTTP caching example with large response bodies.
//!
//! This example demonstrates how to use the http-cache-reqwest StreamingCache middleware
//! with large response bodies to test streaming caching performance and behavior.
//!
//! Run with: cargo run --example reqwest_streaming --features streaming

#![cfg(feature = "streaming")]

use futures_util::StreamExt;
use http_cache::{CacheMode, StreamingManager};
use http_cache_reqwest::StreamingCache;
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

// Generate large response content for testing streaming behavior
fn generate_large_content(size_kb: usize) -> String {
    let chunk =
        "This is a sample line of text for testing streaming cache behavior.\n";
    let lines_needed = (size_kb * 1024) / chunk.len();
    chunk.repeat(lines_needed)
}

async fn setup_mock_server() -> MockServer {
    let mock_server = MockServer::start().await;

    // Root endpoint - basic info
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(|_: &wiremock::Request| {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            ResponseTemplate::new(200)
                .set_body_string(format!(
                    "Large Content Cache Demo - Generated at: {timestamp}\n\nThis example tests caching with different payload sizes."
                ))
                .append_header("content-type", "text/plain")
                .append_header("cache-control", "max-age=60, public")
        })
        .mount(&mock_server)
        .await;

    // Small content endpoint - 1KB
    Mock::given(method("GET"))
        .and(path("/small"))
        .respond_with(|_: &wiremock::Request| {
            let content = generate_large_content(1); // 1KB
            let timestamp =
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

            println!("Generated small content ({} bytes)", content.len());

            ResponseTemplate::new(200)
                .set_body_string(format!(
                    "Small Content (1KB) - Generated at: {}\n{}",
                    timestamp,
                    &content[..200.min(content.len())] // Truncate for readability
                ))
                .append_header("content-type", "text/plain")
                .append_header("cache-control", "max-age=300, public")
                .append_header("x-content-size", &content.len().to_string())
        })
        .mount(&mock_server)
        .await;

    // Large content endpoint - 1MB
    Mock::given(method("GET"))
        .and(path("/large"))
        .respond_with(|_: &wiremock::Request| {
            let content = generate_large_content(1024); // 1MB
            let timestamp =
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

            println!("Generated large content ({} bytes)", content.len());

            ResponseTemplate::new(200)
                .set_body_string(format!(
                    "Large Content (1MB) - Generated at: {}\n{}",
                    timestamp,
                    &content[..500.min(content.len())] // Truncate for readability
                ))
                .append_header("content-type", "text/plain")
                .append_header("cache-control", "max-age=600, public")
                .append_header("x-content-size", &content.len().to_string())
        })
        .mount(&mock_server)
        .await;

    // Huge content endpoint - 5MB
    Mock::given(method("GET"))
        .and(path("/huge"))
        .respond_with(|_: &wiremock::Request| {
            let content = generate_large_content(5120); // 5MB
            let timestamp =
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

            println!("Generated huge content ({} bytes)", content.len());

            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_millis(200))
                .set_body_string(format!(
                    "Huge Content (5MB) - Generated at: {}\n{}",
                    timestamp,
                    &content[..1000.min(content.len())] // Truncate for readability
                ))
                .append_header("content-type", "text/plain")
                .append_header("cache-control", "max-age=1800, public")
                .append_header("x-content-size", &content.len().to_string())
                .append_header("x-streaming", "true")
        })
        .mount(&mock_server)
        .await;

    // Fresh content endpoint - 512KB, never cached
    Mock::given(method("GET"))
        .and(path("/fresh"))
        .respond_with(|_: &wiremock::Request| {
            let content = generate_large_content(512); // 512KB
            let timestamp =
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

            println!("Generated fresh content ({} bytes)", content.len());

            ResponseTemplate::new(200)
                .set_body_string(format!(
                    "Fresh Content (512KB) - Always Generated at: {}\n{}",
                    timestamp,
                    &content[..300.min(content.len())] // Truncate for readability
                ))
                .append_header("content-type", "text/plain")
                .append_header("cache-control", "no-cache")
                .append_header("x-content-size", &content.len().to_string())
        })
        .mount(&mock_server)
        .await;

    // Large JSON API endpoint
    Mock::given(method("GET"))
        .and(path("/api/data"))
        .respond_with(|_: &wiremock::Request| {
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

            println!("Generated large JSON API response ({} bytes)", json_data.len());

            ResponseTemplate::new(200)
                .set_body_string(json_data)
                .append_header("content-type", "application/json")
                .append_header("cache-control", "max-age=900, public")
        })
        .mount(&mock_server)
        .await;

    // Slow endpoint with large content - 256KB
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(|_: &wiremock::Request| {
            let content = generate_large_content(256); // 256KB

            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_millis(1000))
                .set_body_string(format!(
                    "This was a slow response with large content!\n{}",
                    &content[..400.min(content.len())] // Truncate for readability
                ))
                .append_header("content-type", "text/plain")
                .append_header("cache-control", "max-age=120, public")
                .append_header("x-content-size", &content.len().to_string())
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

    // Print cache-related and content-size headers
    for (name, value) in response.headers() {
        let name_str = name.as_str();
        if name_str.starts_with("cache-")
            || name_str.starts_with("x-cache")
            || name_str.starts_with("x-content")
        {
            if let Ok(value_str) = value.to_str() {
                println!("Header {name}: {value_str}");
            }
        }
    }

    // Get response body length for display using streaming
    let mut body_stream = response.bytes_stream();
    let mut total_bytes = 0;

    while let Some(chunk_result) = body_stream.next().await {
        let chunk = chunk_result?;
        total_bytes += chunk.len();
    }

    println!("Response body size: {total_bytes} bytes (streamed)");
    println!("Response received successfully");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("HTTP Cache Reqwest Example - Large Content Streaming Testing");
    println!("============================================================");

    // Set up mock server
    let mock_server = setup_mock_server().await;
    let base_url = mock_server.uri();

    // Create streaming cache manager with disk storage
    let cache_dir = tempdir()?;
    let streaming_manager =
        StreamingManager::new(cache_dir.path().to_path_buf());

    // Create the streaming cache
    let streaming_cache =
        StreamingCache::new(streaming_manager, CacheMode::Default);

    // Build the client with streaming caching middleware
    let client =
        ClientBuilder::new(Client::new()).with(streaming_cache).build();

    println!(
        "Demonstrating HTTP streaming caching with large response bodies...\n"
    );

    // Scenario 1: Small content caching
    make_request(
        &client,
        &format!("{base_url}/small"),
        "Small content (1KB) - First request",
    )
    .await?;
    make_request(
        &client,
        &format!("{base_url}/small"),
        "Small content (1KB) - Second request (should be cached)",
    )
    .await?;

    // Scenario 2: Large content caching
    make_request(
        &client,
        &format!("{base_url}/large"),
        "Large content (1MB) - First request",
    )
    .await?;
    make_request(
        &client,
        &format!("{base_url}/large"),
        "Large content (1MB) - Second request (should be cached)",
    )
    .await?;

    // Scenario 3: Huge content caching (this will take longer to generate and cache)
    make_request(
        &client,
        &format!("{base_url}/huge"),
        "Huge content (5MB) - First request",
    )
    .await?;
    make_request(
        &client,
        &format!("{base_url}/huge"),
        "Huge content (5MB) - Second request (should be cached)",
    )
    .await?;

    // Scenario 4: Non-cacheable large content
    make_request(
        &client,
        &format!("{base_url}/fresh"),
        "Fresh content (512KB) - First request",
    )
    .await?;
    make_request(
        &client,
        &format!("{base_url}/fresh"),
        "Fresh content (512KB) - Second request (always fresh)",
    )
    .await?;

    // Scenario 5: Large JSON API response
    make_request(
        &client,
        &format!("{base_url}/api/data"),
        "Large JSON API - First request",
    )
    .await?;
    make_request(
        &client,
        &format!("{base_url}/api/data"),
        "Large JSON API - Second request (should be cached)",
    )
    .await?;

    // Scenario 6: Slow endpoint with large content
    make_request(
        &client,
        &format!("{base_url}/slow"),
        "Slow endpoint with large content (first request)",
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
