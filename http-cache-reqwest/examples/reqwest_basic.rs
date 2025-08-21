//! Basic HTTP caching with reqwest
//!
//! Run with: cargo run --example reqwest_basic --features manager-cacache

use http_cache::{CacheMode, HttpCache, HttpCacheOptions};
use http_cache_reqwest::{CACacheManager, Cache};
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use std::time::Instant;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Setup mock server with cacheable response
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("Hello from cached response!")
                .append_header("cache-control", "max-age=300, public")
                .append_header("content-type", "text/plain"),
        )
        .mount(&mock_server)
        .await;

    let cache_dir = tempfile::tempdir().unwrap();
    let cache_manager =
        CACacheManager::new(cache_dir.path().to_path_buf(), true);
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: cache_manager,
            options: HttpCacheOptions::default(),
        }))
        .build();

    let url = format!("{}/", mock_server.uri());

    // First request
    let start = Instant::now();
    let response = client.get(&url).send().await?;

    println!("First request: {:?}", start.elapsed());
    println!("Status: {}", response.status());

    // Second request
    let start = Instant::now();
    let response = client.get(&url).send().await?;

    println!("Second request: {:?}", start.elapsed());
    println!("Status: {}", response.status());

    // Check cache headers
    if let Some(cache_control) = response.headers().get("cache-control") {
        println!("Cache header cache-control: {:?}", cache_control);
    }
    if let Some(x_cache) = response.headers().get("x-cache") {
        println!("Cache header x-cache: {:?}", x_cache);
    }
    if let Some(x_cache_lookup) = response.headers().get("x-cache-lookup") {
        println!("Cache header x-cache-lookup: {:?}", x_cache_lookup);
    }

    Ok(())
}
