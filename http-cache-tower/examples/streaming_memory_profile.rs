//! Streaming memory profiling example
//!
//! This example demonstrates and compares memory usage between buffered and streaming cache
//! implementations when handling large responses. It's only available when the
//! "streaming" feature is enabled.
//!
//! Run with: cargo run --example streaming_memory_profile --features streaming

#![cfg(feature = "streaming")]

use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use http_cache::{CACacheManager, FileCacheManager};
use http_cache_tower::{HttpCacheLayer, HttpCacheStreamingLayer};
use std::alloc::{GlobalAlloc, Layout, System};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use tower::{Layer, Service, ServiceExt};

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

// Service that generates large responses
#[derive(Clone)]
struct LargeResponseService {
    size: usize,
}

impl LargeResponseService {
    fn new(size: usize) -> Self {
        Self { size }
    }
}

impl Service<Request<Full<Bytes>>> for LargeResponseService {
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

    fn call(&mut self, _req: Request<Full<Bytes>>) -> Self::Future {
        let size = self.size;

        Box::pin(async move {
            // Create large response data
            let data = vec![b'X'; size];

            let response = Response::builder()
                .status(StatusCode::OK)
                .header("cache-control", "max-age=3600, public")
                .header("content-type", "application/octet-stream")
                .header("content-length", size.to_string())
                .body(Full::new(Bytes::from(data)))
                .map_err(|e| {
                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                })?;

            Ok(response)
        })
    }
}

async fn measure_cache_hit_memory_usage(
    payload_size: usize,
    is_streaming: bool,
) -> (usize, usize, usize) {
    // Create a temporary directory for the cache
    let temp_dir = tempfile::tempdir().unwrap();

    if is_streaming {
        let file_cache_manager =
            FileCacheManager::new(temp_dir.path().to_path_buf());
        let streaming_layer = HttpCacheStreamingLayer::new(file_cache_manager);
        let service = LargeResponseService::new(payload_size);
        let cached_service = streaming_layer.layer(service);

        // First request to populate cache
        let request1 = Request::builder()
            .uri("https://example.com/cache-hit-test")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let _ = cached_service
            .clone()
            .oneshot(request1)
            .await
            .unwrap()
            .into_body()
            .collect()
            .await;

        // Reset memory tracking before cache hit test
        MEMORY_TRACKER.reset();
        let initial_memory = MEMORY_TRACKER.current_usage();

        // Second request (cache hit)
        let request2 = Request::builder()
            .uri("https://example.com/cache-hit-test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let response = cached_service.oneshot(request2).await.unwrap();
        let peak_after_response = MEMORY_TRACKER.current_usage();

        // Stream from cached file
        let body = response.into_body();
        let mut peak_during_streaming = peak_after_response;

        let mut body_stream = std::pin::pin!(body);
        while let Some(frame_result) = body_stream.frame().await {
            let frame = frame_result.unwrap();
            if let Some(_chunk) = frame.data_ref() {
                let current_memory = MEMORY_TRACKER.current_usage();
                peak_during_streaming =
                    peak_during_streaming.max(current_memory);
            }
        }

        let peak_after_consumption = MEMORY_TRACKER.current_usage();

        (
            peak_after_response - initial_memory,
            peak_during_streaming - initial_memory,
            peak_after_consumption - initial_memory,
        )
    } else {
        let cache_manager =
            CACacheManager::new(temp_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager);
        let service = LargeResponseService::new(payload_size);
        let cached_service = cache_layer.layer(service);

        // First request to populate cache
        let request1 = Request::builder()
            .uri("https://example.com/cache-hit-test")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let _ = cached_service
            .clone()
            .oneshot(request1)
            .await
            .unwrap()
            .into_body()
            .collect()
            .await;

        // Reset memory tracking before cache hit test
        MEMORY_TRACKER.reset();
        let initial_memory = MEMORY_TRACKER.current_usage();

        // Second request (cache hit)
        let request2 = Request::builder()
            .uri("https://example.com/cache-hit-test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let response = cached_service.oneshot(request2).await.unwrap();
        let peak_after_response = MEMORY_TRACKER.current_usage();

        // Stream cached response (likely loaded from disk into memory)
        let body = response.into_body();
        let mut peak_during_streaming = peak_after_response;

        let mut body_stream = std::pin::pin!(body);
        while let Some(frame_result) = body_stream.frame().await {
            let frame = frame_result.unwrap();
            if let Some(_chunk) = frame.data_ref() {
                let current_memory = MEMORY_TRACKER.current_usage();
                peak_during_streaming =
                    peak_during_streaming.max(current_memory);
            }
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
    println!("Memory Usage Analysis: Buffered vs Streaming Cache");
    println!("==================================================");
    println!("This analysis measures memory efficiency differences between");
    println!("traditional buffered caching and file-based streaming caching.");
    println!("Key differences appear during cache hits where streaming");
    println!("serves responses directly from disk without memory buffering.\n");

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
        println!("  Response likely loaded into memory from cache storage");

        // Test streaming cache hit
        let (streaming_response, streaming_peak, streaming_final) =
            measure_cache_hit_memory_usage(*size, true).await;

        println!("\nStreaming Cache Hit ({}KB payload):", size / 1024);
        println!("  Response memory delta: {streaming_response} bytes");
        println!("  Peak memory delta: {streaming_peak} bytes");
        println!("  Final memory delta: {streaming_final} bytes");
        println!("  Response streamed directly from disk file, never loaded into memory");

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
