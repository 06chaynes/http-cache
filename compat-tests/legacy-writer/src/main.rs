use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use std::path::PathBuf;

/// Writes cache entries using http-cache-reqwest 0.16.0 (bincode serialization).
/// Used to validate the upgrade path from 0.16 -> 1.0-alpha.
///
/// Usage: cargo run -- <cache-directory>
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/legacy-cache"));

    println!("Writing legacy cache entries to: {}", cache_dir.display());

    let manager = CACacheManager {
        path: cache_dir.clone(),
        remove_opts: cacache::RemoveOpts::new().remove_fully(true),
    };

    let client = ClientBuilder::new(Client::builder().build()?)
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager,
            options: HttpCacheOptions::default(),
        }))
        .build();

    let urls = [
        "https://httpbin.org/cache/300",
        "https://httpbin.org/response-headers?Cache-Control=public%2Cmax-age%3D3600",
        "https://jsonplaceholder.typicode.com/posts/1",
    ];

    for url in &urls {
        println!("\nRequesting: {}", url);
        match client.get(*url).send().await {
            Ok(response) => {
                println!("  Status: {}", response.status());
                println!(
                    "  Cache-Control: {:?}",
                    response.headers().get("cache-control")
                );
                let body = response.bytes().await?;
                println!("  Body length: {} bytes", body.len());
            }
            Err(e) => {
                eprintln!("  Error: {}", e);
            }
        }
    }

    println!("\nCache entries written to: {}", cache_dir.display());
    println!("Now run the read-legacy-cache tool to verify migration.");
    Ok(())
}
