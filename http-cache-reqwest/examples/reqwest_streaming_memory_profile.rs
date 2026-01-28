//! Streaming memory profiling example for reqwest
//!
//! This example demonstrates and compares memory usage between buffered and streaming cache
//! implementations when handling large responses. It's only available when the
//! "streaming" feature is enabled.
//!
//! Run with: cargo run --example streaming_memory_profile --features streaming

#![cfg(feature = "streaming")]

use futures_util::StreamExt;
use http_cache::{CACacheManager, StreamingManager};
use http_cache_reqwest::{Cache, StreamingCache};
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

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

async fn create_mock_server(payload_size: usize) -> MockServer {
    let mock_server = MockServer::start().await;
    let large_body = vec![b'X'; payload_size];

    Mock::given(method("GET"))
        .and(path("/large-response"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(large_body)
                .append_header("cache-control", "max-age=3600, public")
                .append_header("content-type", "application/octet-stream"),
        )
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
        // Create streaming cache setup using StreamingManager
        let streaming_manager = StreamingManager::with_temp_dir(1000)
            .await
            .expect("Failed to create streaming manager");
        let streaming_cache = StreamingCache::new(
            streaming_manager,
            http_cache::CacheMode::Default,
        );

        let client = ClientBuilder::new(reqwest::Client::new())
            .with(streaming_cache)
            .build();

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

        // Stream response body properly using bytes_stream()
        let mut body_stream = response2.bytes_stream();
        let mut peak_during_streaming = peak_after_response;

        while let Some(chunk_result) = body_stream.next().await {
            let _chunk = chunk_result.unwrap();
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
        let temp_dir = tempdir().unwrap();
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

        // Buffer response body (non-streaming test)
        let body_bytes = response2.bytes().await.unwrap();
        let mut peak_during_streaming = peak_after_response;

        // Simulate chunk processing to track memory during buffering
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

async fn run_memory_analysis() {
    println!("Memory Usage Analysis: Buffered vs Streaming Cache (Reqwest)");
    println!("============================================================");
    println!("This analysis measures memory efficiency differences between");
    println!("traditional buffered caching and file-based streaming caching.");
    println!("Measurements are taken during cache hits to compare memory usage patterns.");
    println!();

    let payload_sizes = [
        (100 * 1024, "100KB"),
        (1024 * 1024, "1024KB"),
        (5 * 1024 * 1024, "5120KB"),
        (10 * 1024 * 1024, "10240KB"),
    ];

    let mut max_buffered_peak = 0;
    let mut max_streaming_peak = 0;

    for (size, size_label) in payload_sizes {
        println!("Testing cache hits with {size_label} payload:");
        println!(
            "============================================================"
        );

        // Test buffered cache
        let (buffered_response, buffered_peak, buffered_final) =
            measure_cache_hit_memory_usage(size, false).await;

        println!("Buffered Cache Hit ({size_label} payload):");
        println!("  Response memory delta: {buffered_response} bytes");
        println!("  Peak memory delta: {buffered_peak} bytes");
        println!("  Final memory delta: {buffered_final} bytes");
        println!();

        max_buffered_peak = max_buffered_peak.max(buffered_peak);

        // Test streaming cache
        let (streaming_response, streaming_peak, streaming_final) =
            measure_cache_hit_memory_usage(size, true).await;

        println!("Streaming Cache Hit ({size_label} payload):");
        println!("  Response memory delta: {streaming_response} bytes");
        println!("  Peak memory delta: {streaming_peak} bytes");
        println!("  Final memory delta: {streaming_final} bytes");
        println!();

        max_streaming_peak = max_streaming_peak.max(streaming_peak);

        // Compare results
        println!("Cache hit memory comparison:");
        if streaming_response <= buffered_response {
            let savings = ((buffered_response - streaming_response) as f64
                / buffered_response as f64)
                * 100.0;
            println!(
                "  Response memory savings: {savings:.1}% ({buffered_response} vs {streaming_response} bytes)"
            );
        } else {
            let increase = ((streaming_response - buffered_response) as f64
                / buffered_response as f64)
                * 100.0;
            println!(
                "  Response memory increase: {increase:.1}% ({buffered_response} vs {streaming_response} bytes)"
            );
        }

        if streaming_peak <= buffered_peak {
            let savings = ((buffered_peak - streaming_peak) as f64
                / buffered_peak as f64)
                * 100.0;
            println!(
                "  Peak memory savings: {savings:.1}% ({buffered_peak} vs {streaming_peak} bytes)"
            );
        } else {
            let increase = ((streaming_peak - buffered_peak) as f64
                / buffered_peak as f64)
                * 100.0;
            println!(
                "  Peak memory increase: {increase:.1}% ({buffered_peak} vs {streaming_peak} bytes)"
            );
        }

        if streaming_final <= buffered_final {
            let savings = ((buffered_final - streaming_final) as f64
                / buffered_final as f64)
                * 100.0;
            println!(
                "  Final memory savings: {savings:.1}% ({buffered_final} vs {streaming_final} bytes)"
            );
        } else {
            let increase = ((streaming_final - buffered_final) as f64
                / buffered_final as f64)
                * 100.0;
            println!(
                "  Final memory increase: {increase:.1}% ({buffered_final} vs {streaming_final} bytes)"
            );
        }

        let abs_diff = buffered_peak.abs_diff(streaming_peak);
        println!("  Absolute memory difference: {abs_diff} bytes");
        println!();
        println!();
    }

    // Overall summary
    println!("Overall Analysis Summary:");
    println!("========================");
    println!("Max buffered peak memory: {max_buffered_peak} bytes");
    println!("Max streaming peak memory: {max_streaming_peak} bytes");
    let overall_savings = if max_streaming_peak <= max_buffered_peak {
        ((max_buffered_peak - max_streaming_peak) as f64
            / max_buffered_peak as f64)
            * 100.0
    } else {
        -((max_streaming_peak - max_buffered_peak) as f64
            / max_buffered_peak as f64)
            * 100.0
    };
    println!("Overall memory savings: {overall_savings:.1}%");
}

#[tokio::main]
async fn main() {
    run_memory_analysis().await;
}
