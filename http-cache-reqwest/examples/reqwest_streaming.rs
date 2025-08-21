//! Streaming HTTP caching with reqwest
//!
//! Run with: cargo run --example reqwest_streaming --features streaming

#![cfg(feature = "streaming")]

use futures_util::StreamExt;
use http_cache::{CacheMode, StreamingManager};
use http_cache_reqwest::StreamingCache;
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use std::time::Instant;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Setup mock server with streaming cacheable response
    let mock_server = MockServer::start().await;
    let large_content = "X".repeat(10000); // 10KB of data to simulate streaming
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(&large_content)
                .append_header("cache-control", "max-age=300, public")
                .append_header("content-type", "text/plain"),
        )
        .mount(&mock_server)
        .await;

    let cache_dir = tempfile::tempdir().unwrap();
    let streaming_manager =
        StreamingManager::new(cache_dir.path().to_path_buf());

    let client = ClientBuilder::new(Client::new())
        .with(StreamingCache::new(streaming_manager, CacheMode::Default))
        .build();

    let url = format!("{}/", mock_server.uri());

    println!("Testing streaming HTTP caching with reqwest...");

    // First request - will be cached as a stream
    let start = Instant::now();
    let response = client.get(&url).send().await?;
    let duration1 = start.elapsed();

    println!("First request: {:?}", duration1);
    println!("Status: {}", response.status());

    // Read the streaming response
    let mut stream = response.bytes_stream();
    let mut body_size = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        body_size += chunk.len();
    }
    println!("First response body size: {} bytes", body_size);

    // Second request - should be served from cache
    let start = Instant::now();
    let response = client.get(&url).send().await?;
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

    // Read the cached streaming response
    let mut cached_stream = response.bytes_stream();
    let mut cached_body_size = 0;
    while let Some(chunk) = cached_stream.next().await {
        let chunk = chunk?;
        cached_body_size += chunk.len();
    }
    println!("Second response body size: {} bytes", cached_body_size);

    // Verify both responses have the same content
    if cached_body_size != body_size {
        println!("Warning: Content size mismatch");
    }

    Ok(())
}
