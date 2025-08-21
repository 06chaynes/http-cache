//! Basic HTTP caching with ureq
//!
//! Run with: cargo run --example ureq_basic --features manager-cacache

use http_cache_ureq::{CACacheManager, CachedAgent};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    smol::block_on(async {
        let cache_dir = tempfile::tempdir().unwrap();
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), true);
        let client =
            CachedAgent::builder().cache_manager(cache_manager).build()?;

        // First request
        let start = Instant::now();
        let response =
            client.get("https://httpbin.org/cache/300").call().await?;

        println!("First request: {:?}", start.elapsed());
        println!("Status: {}", response.status());

        // Second request
        let start = Instant::now();
        let response =
            client.get("https://httpbin.org/cache/300").call().await?;

        println!("Second request: {:?}", start.elapsed());
        println!("Status: {}", response.status());

        // Check cache headers
        if let Some(cache_control) = response.header("cache-control") {
            println!("Cache header cache-control: {}", cache_control);
        }
        if let Some(x_cache) = response.header("x-cache") {
            println!("Cache header x-cache: {}", x_cache);
        }
        if let Some(x_cache_lookup) = response.header("x-cache-lookup") {
            println!("Cache header x-cache-lookup: {}", x_cache_lookup);
        }

        Ok(())
    })
}
