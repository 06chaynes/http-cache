use http_cache::{CACacheManager, CacheManager};
use std::path::PathBuf;

/// Reads cache entries written by the legacy-writer (http-cache-reqwest 0.16.0)
/// using the current code (1.0-alpha with postcard + bincode fallback).
///
/// Usage: cargo run -- <cache-directory>
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/legacy-cache"));

    println!("Reading legacy cache entries from: {}", cache_dir.display());

    let manager = CACacheManager::new(cache_dir.clone(), true);

    let urls = [
        "https://httpbin.org/cache/300",
        "https://httpbin.org/response-headers?Cache-Control=public%2Cmax-age%3D3600",
        "https://jsonplaceholder.typicode.com/posts/1",
    ];

    let mut success_count = 0;
    let mut miss_count = 0;
    let mut total = 0;

    for url in &urls {
        total += 1;
        let cache_key = format!("GET:{}", url);
        println!("\nLooking up: {}", cache_key);

        match manager.get(&cache_key).await {
            Ok(Some((response, policy))) => {
                success_count += 1;
                println!("  HIT - bincode fallback worked!");
                println!("  Status: {}", response.status);
                println!("  Body length: {} bytes", response.body.len());
                println!("  Headers: {:?}", response.headers);
                println!("  Version: {:?}", response.version);
                println!("  Policy is_stale: {}", policy.is_stale(std::time::SystemTime::now()));
            }
            Ok(None) => {
                miss_count += 1;
                println!("  MISS - entry not found or not deserializable");
            }
            Err(e) => {
                eprintln!("  ERROR: {} (this should NOT happen!)", e);
            }
        }
    }

    println!("\n--- Summary ---");
    println!("Total: {}, Hits: {}, Misses: {}", total, success_count, miss_count);

    if success_count == total {
        println!("All legacy entries were successfully read via bincode fallback!");
    } else if miss_count > 0 && success_count == 0 {
        println!("No entries were readable. This could mean:");
        println!("  - The legacy-writer hasn't been run yet");
        println!("  - The cache directory is wrong");
        println!("  - The bincode fallback isn't working");
    }

    Ok(())
}
