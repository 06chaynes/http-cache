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
use http_body_util::{BodyExt, Full, StreamBody};
use http_cache::{CACacheManager, StreamingBody, StreamingManager};
use http_cache_tower::{HttpCacheLayer, HttpCacheStreamingLayer};
use std::alloc::{GlobalAlloc, Layout, System};
use std::convert::Infallible;
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
    if is_streaming {
        let file_cache_manager = StreamingManager::with_temp_dir(1000)
            .await
            .expect("Failed to create streaming manager");
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
        let temp_dir = tempfile::tempdir().unwrap();
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

        // Stream cached response
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

// --- Issue #164 regression gate ---
//
// `StreamingManager::put` used to `body.collect()` the whole upstream body
// into memory before writing it to disk. This drives a 256MiB response (one
// reused static 64KiB chunk, never a large buffer) through the layer and
// asserts the returned body is the disk-backed `File` variant with a single
// committed entry — a buffering regression would return a `Buffered` body.
// Peak RSS is printed for reference only: it is a whole-process high-water
// mark that swings with the allocator (macOS returns freed memory, glibc
// retains it), so it is not a portable pass/fail signal.

/// Returns this process's peak (high-water-mark) resident set size, in MB.
///
/// `getrusage`'s `ru_maxrss` is reported in bytes on macOS but in KiB on
/// Linux (and most other targets) — the unit differs by platform, not by
/// libc implementation, so this has to be a compile-time `cfg`.
fn peak_rss_mb() -> f64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    assert_eq!(ret, 0, "getrusage failed");

    #[cfg(target_os = "macos")]
    {
        ru.ru_maxrss as f64 / (1024.0 * 1024.0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        ru.ru_maxrss as f64 / 1024.0
    }
}

/// Size of the reused static chunk that makes up the gate's response body.
const GATE_CHUNK_SIZE: usize = 64 * 1024;
static GATE_CHUNK: [u8; GATE_CHUNK_SIZE] = [0u8; GATE_CHUNK_SIZE];
/// 4096 * 64KiB = 256MiB streamed response.
const GATE_CHUNK_COUNT: usize = 4096;

/// The 256MiB gate body must sit under the cap or `put` declines instead of
/// spooling; 512MiB gives 2x headroom.
const GATE_MAX_BODY_SIZE: u64 = 512 * 1024 * 1024;

type GateStream = Pin<
    Box<
        dyn futures_util::Stream<
                Item = Result<http_body::Frame<Bytes>, Infallible>,
            > + Send,
    >,
>;
type GateBody = StreamBody<GateStream>;

fn gate_stream() -> GateStream {
    Box::pin(futures_util::stream::iter(
        (0..GATE_CHUNK_COUNT).map(|_| {
            Ok(http_body::Frame::data(Bytes::from_static(&GATE_CHUNK)))
        }),
    ))
}

/// A service that returns one cacheable response streamed from
/// `GATE_CHUNK_COUNT` copies of `GATE_CHUNK`, without ever holding more than
/// one chunk in memory at a time.
#[derive(Clone)]
struct GateService;

impl Service<Request<Full<Bytes>>> for GateService {
    type Response = Response<GateBody>;
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
        Box::pin(async move {
            let total_len = GATE_CHUNK_COUNT * GATE_CHUNK_SIZE;
            let response = Response::builder()
                .status(StatusCode::OK)
                .header("cache-control", "max-age=3600, public")
                .header("content-type", "application/octet-stream")
                .header("content-length", total_len.to_string())
                .body(StreamBody::new(gate_stream()))
                .map_err(|e| {
                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                })?;
            Ok(response)
        })
    }
}

/// Runs the issue-#164 regression gate. Exits the process non-zero (rather
/// than returning a failure indication) so `just memory-profile` fails
/// loudly and visibly on a memory regression.
async fn run_issue_164_regression_gate() {
    let total_mb = (GATE_CHUNK_COUNT * GATE_CHUNK_SIZE) / (1024 * 1024);
    println!("Issue #164 regression gate");
    println!("===========================");
    println!(
        "Streaming a {total_mb}MiB cacheable response through \
         HttpCacheStreamingLayer (cache miss -> StreamingManager::put)..."
    );

    let cache_dir = tempfile::tempdir()
        .expect("failed to create temp dir for streaming manager");
    let manager = StreamingManager::with_max_body_size(
        cache_dir.path().to_path_buf(),
        1000,
        GATE_MAX_BODY_SIZE,
    )
    .await
    .expect("failed to create streaming manager");
    // Clone shares the same database + moka handle as the layer's manager.
    let manager_handle = manager.clone();
    let cache_layer = HttpCacheStreamingLayer::new(manager);
    let cached_service = cache_layer.layer(GateService);

    let request = Request::builder()
        .uri("https://example.com/issue-164-gate")
        .body(Full::new(Bytes::new()))
        .unwrap();

    assert_eq!(
        manager_handle.entry_count(),
        0,
        "sanity check: cache must be empty before the gate request"
    );

    let response = cached_service.oneshot(request).await.unwrap();
    assert!(
        matches!(response.body(), StreamingBody::File { .. }),
        "expected a disk-backed File body — put() declined to cache instead \
         of committing"
    );
    let body = response.into_body();
    let mut body_stream = std::pin::pin!(body);
    let mut total_bytes = 0usize;
    while let Some(frame_result) = body_stream.frame().await {
        let frame = frame_result.unwrap();
        if let Some(chunk) = frame.data_ref() {
            total_bytes += chunk.len();
        }
    }
    assert_eq!(total_bytes, GATE_CHUNK_COUNT * GATE_CHUNK_SIZE);

    // Proof the entry was committed, not just served from the spool handle.
    // entry_count() is only accurate after run_pending_tasks.
    manager_handle.run_pending_tasks().await;
    assert_eq!(
        manager_handle.entry_count(),
        1,
        "expected exactly one committed cache entry after the gate request"
    );
    println!(
        "  cache entry_count() after request: {} (commit confirmed)",
        manager_handle.entry_count()
    );

    println!(
        "  peak RSS: {:.1} MB (informational) for a {total_mb}MiB streamed \
         response",
        peak_rss_mb()
    );
    println!("  PASS: streamed to disk and committed one entry.\n");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Measure first: peak RSS is a whole-process high-water mark.
    run_issue_164_regression_gate().await;

    println!("Memory Usage Analysis: Buffered vs Streaming Cache");
    println!("==================================================");
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
