//! Streaming memory profiling example for reqwest
//!
//! This example demonstrates and compares memory usage between buffered and streaming cache
//! implementations when handling large responses. It's only available when the
//! "streaming" feature is enabled.
//!
//! Run with: cargo run --example streaming_memory_profile --features streaming

#![cfg(feature = "streaming")]

use http_cache::{CACacheManager, FileCacheManager};
use http_cache_reqwest::{Cache, StreamingCache};
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// Memory tracking allocator
struct MemoryTracker {
    allocations: AtomicUsize,
}

impl MemoryTracker {
    const fn new() -> Self {
        Self { allocations: AtomicUsize::new(0) }
    }

    fn current_usage(&self) -> usize {
        self.allocations.load(Ordering::Relaxed)
    }

    fn reset(&self) {
        self.allocations.store(0, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for MemoryTracker {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            self.allocations.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        self.allocations.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static MEMORY_TRACKER: MemoryTracker = MemoryTracker::new();

// Create a mock server that serves large responses
async fn create_mock_server(payload_size: usize) -> MockServer {
    let mock_server = MockServer::start().await;

    let large_body = vec![b'X'; payload_size];
    let response = ResponseTemplate::new(200)
        .set_body_bytes(large_body)
        .append_header("cache-control", "max-age=3600, public")
        .append_header("content-type", "application/octet-stream");

    Mock::given(method("GET"))
        .and(path("/large-response"))
        .respond_with(response)
        .mount(&mock_server)
        .await;

    mock_server
}

async fn measure_cache_hit_memory_usage(
    payload_size: usize,
    is_streaming: bool,
) -> (usize, usize, usize) {
    let mock_server = create_mock_server(payload_size).await;
    let url = format!("{}/large-response", mock_server.uri());

    if is_streaming {
        // Create streaming cache setup
        let temp_dir = tempfile::tempdir().unwrap();
        let file_cache_manager =
            FileCacheManager::new(temp_dir.path().to_path_buf());
        let streaming_cache = StreamingCache::new(
            file_cache_manager,
            http_cache::CacheMode::Default,
        );

        let client: ClientWithMiddleware =
            ClientBuilder::new(Client::new()).with(streaming_cache).build();

        // First request to populate cache
        let _response1 = client.get(&url).send().await.unwrap();
        let _body1 = _response1.bytes().await.unwrap();

        // Wait a moment to ensure cache is written
        sleep(Duration::from_millis(100)).await;

        // Reset memory tracking before cache hit test
        MEMORY_TRACKER.reset();
        let initial_memory = MEMORY_TRACKER.current_usage();

        // Second request (cache hit)
        let response2 = client.get(&url).send().await.unwrap();
        let peak_after_response = MEMORY_TRACKER.current_usage();

        // Stream response body
        let mut peak_during_streaming = peak_after_response;
        let body_bytes = response2.bytes().await.unwrap();

        // Simulate chunk processing to track memory during streaming
        for chunk in body_bytes.chunks(8192) {
            let _processed_chunk = chunk;
            let current_memory = MEMORY_TRACKER.current_usage();
            peak_during_streaming = peak_during_streaming.max(current_memory);
        }

        let peak_after_consumption = MEMORY_TRACKER.current_usage();

        (
            peak_after_response - initial_memory,
            peak_during_streaming - initial_memory,
            peak_after_consumption - initial_memory,
        )
    } else {
        // Create buffered cache setup
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_manager =
            CACacheManager::new(temp_dir.path().to_path_buf(), false);
        let cache = Cache(http_cache::HttpCache {
            mode: http_cache::CacheMode::Default,
            manager: cache_manager,
            options: http_cache::HttpCacheOptions::default(),
        });

        let client: ClientWithMiddleware =
            ClientBuilder::new(Client::new()).with(cache).build();

        // First request to populate cache
        let _response1 = client.get(&url).send().await.unwrap();
        let _body1 = _response1.bytes().await.unwrap();

        // Wait a moment to ensure cache is written
        sleep(Duration::from_millis(100)).await;

        // Reset memory tracking before cache hit test
        MEMORY_TRACKER.reset();
        let initial_memory = MEMORY_TRACKER.current_usage();

        // Second request (cache hit)
        let response2 = client.get(&url).send().await.unwrap();
        let peak_after_response = MEMORY_TRACKER.current_usage();

        // Read response body
        let mut peak_during_streaming = peak_after_response;
        let body_bytes = response2.bytes().await.unwrap();

        // Simulate chunk processing to track memory during streaming
        for chunk in body_bytes.chunks(8192) {
            let _processed_chunk = chunk;
            let current_memory = MEMORY_TRACKER.current_usage();
            peak_during_streaming = peak_during_streaming.max(current_memory);
        }

        let peak_after_consumption = MEMORY_TRACKER.current_usage();

        (
            peak_after_response - initial_memory,
            peak_during_streaming - initial_memory,
            peak_after_consumption - initial_memory,
        )
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Memory Usage Analysis: Buffered vs Streaming Cache (Reqwest)");
    println!("============================================================");
    println!("This analysis measures memory efficiency differences between");
    println!("traditional buffered caching and file-based streaming caching.");
    println!("Measurements are taken during cache hits to compare memory usage patterns.\n");

    // Memory profiling analysis for different payload sizes
    let payload_sizes = vec![
        100 * 1024,       // 100KB
        1024 * 1024,      // 1MB
        5 * 1024 * 1024,  // 5MB
        10 * 1024 * 1024, // 10MB
    ];

    let mut overall_buffered_peak = 0;
    let mut overall_streaming_peak = 0;

    for size in &payload_sizes {
        println!("Testing cache hits with {}KB payload:", size / 1024);
        println!("{}", "=".repeat(60));

        // Test buffered cache hit
        let (buffered_response, buffered_peak, buffered_final) =
            measure_cache_hit_memory_usage(*size, false).await;

        println!("Buffered Cache Hit ({}KB payload):", size / 1024);
        println!("  Response memory delta: {buffered_response} bytes");
        println!("  Peak memory delta: {buffered_peak} bytes");
        println!("  Final memory delta: {buffered_final} bytes");

        // Test streaming cache hit
        let (streaming_response, streaming_peak, streaming_final) =
            measure_cache_hit_memory_usage(*size, true).await;

        println!("\nStreaming Cache Hit ({}KB payload):", size / 1024);
        println!("  Response memory delta: {streaming_response} bytes");
        println!("  Peak memory delta: {streaming_peak} bytes");
        println!("  Final memory delta: {streaming_final} bytes");

        println!("\nCache hit memory comparison:");

        if buffered_response > 0 && streaming_response < buffered_response {
            let response_savings = ((buffered_response - streaming_response)
                as f64
                / buffered_response as f64)
                * 100.0;
            println!(
                "  Response memory savings: {response_savings:.1}% ({buffered_response} vs {streaming_response} bytes)"
            );
        }

        if buffered_peak > 0 && streaming_peak < buffered_peak {
            let peak_savings = ((buffered_peak - streaming_peak) as f64
                / buffered_peak as f64)
                * 100.0;
            println!(
                "  Peak memory savings: {peak_savings:.1}% ({buffered_peak} vs {streaming_peak} bytes)"
            );
        } else if streaming_peak > buffered_peak {
            let peak_increase = ((streaming_peak - buffered_peak) as f64
                / buffered_peak as f64)
                * 100.0;
            println!(
                "  Peak memory increase: {peak_increase:.1}% ({buffered_peak} vs {streaming_peak} bytes)"
            );
        }

        if buffered_final > 0 && streaming_final < buffered_final {
            let final_savings = ((buffered_final - streaming_final) as f64
                / buffered_final as f64)
                * 100.0;
            println!(
                "  Final memory savings: {final_savings:.1}% ({buffered_final} vs {streaming_final} bytes)"
            );
        }

        println!(
            "  Absolute memory difference: {} bytes",
            (buffered_peak as i64 - streaming_peak as i64).abs()
        );

        overall_buffered_peak = overall_buffered_peak.max(buffered_peak);
        overall_streaming_peak = overall_streaming_peak.max(streaming_peak);

        println!("\n");
    }

    println!("Overall Analysis Summary:");
    println!("========================");
    println!("Max buffered peak memory: {overall_buffered_peak} bytes");
    println!("Max streaming peak memory: {overall_streaming_peak} bytes");

    if overall_buffered_peak > 0
        && overall_streaming_peak < overall_buffered_peak
    {
        let overall_savings = ((overall_buffered_peak - overall_streaming_peak)
            as f64
            / overall_buffered_peak as f64)
            * 100.0;
        println!("Overall memory savings: {overall_savings:.1}%");
    }

    Ok(())
}
