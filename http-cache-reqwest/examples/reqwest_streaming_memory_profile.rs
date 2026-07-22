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

// --- Issue #164 regression gate ---
//
// `StreamingManager::put` used to `body.collect()` the whole upstream body
// into memory before writing it to disk. This drives a real 256MiB response
// (one reused static 64KiB chunk, never a large buffer) over a real HTTP
// connection and fails if process peak RSS exceeds GATE_THRESHOLD_MB. Kept
// out of the unit suite: RSS assertions flake on loaded CI runners.

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
/// ~8x headroom over the observed post-fix peak (~7.7MB on macOS for this
/// 256MiB response). Pre-fix, `put()` collected the whole body first, which
/// pushes peak RSS past 256MB.
const GATE_THRESHOLD_MB: f64 = 64.0;

/// The 256MiB gate body must sit under the cap or `put` declines mid-spool
/// instead of committing; 512MiB gives 2x headroom.
const GATE_MAX_BODY_SIZE: u64 = 512 * 1024 * 1024;

fn gate_body_stream(
) -> impl futures_util::Stream<Item = Result<bytes::Bytes, std::convert::Infallible>>
{
    futures_util::stream::iter((0..GATE_CHUNK_COUNT).map(|_| {
        Ok::<_, std::convert::Infallible>(bytes::Bytes::from_static(
            &GATE_CHUNK,
        ))
    }))
}

/// Starts a minimal local HTTP server that streams `GATE_CHUNK_COUNT`
/// copies of `GATE_CHUNK` as one cacheable response, without ever holding
/// more than one chunk in memory at a time. Returns the bound address.
async fn serve_gate_body() -> std::net::SocketAddr {
    use axum::{body::Body, response::Response, routing::get, Router};

    let app = Router::new().route(
        "/gate",
        get(|| async {
            Response::builder()
                .header("cache-control", "public, max-age=3600")
                .header("content-type", "application/octet-stream")
                .body(Body::from_stream(gate_body_stream()))
                .unwrap()
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

/// Runs the issue-#164 regression gate. Exits the process non-zero (rather
/// than returning a failure indication) so `just memory-profile` fails
/// loudly and visibly on a memory regression.
async fn run_issue_164_regression_gate() {
    let total_mb = (GATE_CHUNK_COUNT * GATE_CHUNK_SIZE) / (1024 * 1024);
    println!("Issue #164 regression gate");
    println!("===========================");
    println!(
        "Streaming a {total_mb}MiB cacheable response through StreamingCache \
         (cache miss -> StreamingManager::put)..."
    );

    let addr = serve_gate_body().await;
    let url = format!("http://{addr}/gate");

    let cache_dir =
        tempdir().expect("failed to create temp dir for streaming manager");
    let streaming_manager = StreamingManager::with_max_body_size(
        cache_dir.path().to_path_buf(),
        1000,
        GATE_MAX_BODY_SIZE,
    )
    .await
    .expect("failed to create streaming manager");
    // Clone shares the same database + moka handle as the middleware's.
    let manager_handle = streaming_manager.clone();
    let streaming_cache =
        StreamingCache::new(streaming_manager, http_cache::CacheMode::Default);
    let client = ClientBuilder::new(reqwest::Client::new())
        .with(streaming_cache)
        .build();

    assert_eq!(
        manager_handle.entry_count(),
        0,
        "sanity check: cache must be empty before the gate request"
    );

    let response = client.get(&url).send().await.expect("request failed");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let mut body_stream = response.bytes_stream();
    let mut total_bytes = 0usize;
    while let Some(chunk) = body_stream.next().await {
        total_bytes += chunk.expect("body stream error").len();
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

    let peak = peak_rss_mb();
    println!(
        "  measured peak RSS: {peak:.1} MB (threshold: {GATE_THRESHOLD_MB:.1} MB)"
    );

    if peak > GATE_THRESHOLD_MB {
        eprintln!(
            "ISSUE #164 REGRESSION: peak RSS {peak:.1} MB exceeds the \
             {GATE_THRESHOLD_MB:.1} MB threshold for a {total_mb}MiB streamed \
             response. StreamingManager::put (or StreamingCache) may be \
             buffering the body again instead of spooling it frame-by-frame."
        );
        std::process::exit(1);
    }

    println!("  PASS: within threshold.\n");
}

#[tokio::main]
async fn main() {
    // Measure first: peak RSS is a whole-process high-water mark.
    run_issue_164_regression_gate().await;
    run_memory_analysis().await;
}
