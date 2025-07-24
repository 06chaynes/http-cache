use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use http_cache::{CACacheManager, FileCacheManager};
use http_cache_tower::{HttpCacheLayer, HttpCacheStreamingLayer};
use std::future::Future;
use std::hint::black_box;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service, ServiceExt};

// Test service that generates responses of specified size
#[derive(Clone)]
struct TestResponseService {
    size: usize,
}

impl TestResponseService {
    fn new(size: usize) -> Self {
        Self { size }
    }
}

impl Service<Request<Full<Bytes>>> for TestResponseService {
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
            let data = vec![b'B'; size];
            let response = Response::builder()
                .status(StatusCode::OK)
                .header("cache-control", "max-age=3600, public")
                .header("content-type", "application/octet-stream")
                .body(Full::new(Bytes::from(data)))
                .map_err(|e| {
                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                })?;
            Ok(response)
        })
    }
}

// Benchmark cache miss performance - buffered vs streaming
fn bench_cache_miss_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_miss_comparison");
    group.sample_size(30);
    group.measurement_time(std::time::Duration::from_secs(10));

    let sizes = vec![
        ("1kb", 1024),
        ("10kb", 10 * 1024),
        ("100kb", 100 * 1024),
        ("1mb", 1024 * 1024),
    ];

    for (size_name, size_bytes) in sizes {
        // Buffered benchmark
        group.bench_with_input(
            BenchmarkId::new("buffered", size_name),
            &size_bytes,
            |b, &size| {
                b.iter_custom(|iters| {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                        let start = std::time::Instant::now();

                        for i in 0..iters {
                            let temp_dir = tempfile::tempdir().unwrap();
                            let cache_manager = CACacheManager::new(
                                temp_dir.path().to_path_buf(),
                                false,
                            );
                            let layer = HttpCacheLayer::new(cache_manager);
                            let service = TestResponseService::new(size);
                            let cached_service = layer.layer(service);

                            let request = Request::builder()
                                .uri(format!(
                                    "https://example.com/miss-test-{i}"
                                ))
                                .body(Full::new(Bytes::new()))
                                .unwrap();

                            let response =
                                cached_service.oneshot(request).await.unwrap();
                            let _body = black_box(
                                response.into_body().collect().await.unwrap(),
                            );
                        }

                        start.elapsed()
                    })
                })
            },
        );

        // Streaming benchmark
        group.bench_with_input(
            BenchmarkId::new("streaming", size_name),
            &size_bytes,
            |b, &size| {
                b.iter_custom(|iters| {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                        let start = std::time::Instant::now();

                        for i in 0..iters {
                            let temp_dir = tempfile::tempdir().unwrap();
                            let cache_manager = FileCacheManager::new(
                                temp_dir.path().to_path_buf(),
                            );
                            let layer =
                                HttpCacheStreamingLayer::new(cache_manager);
                            let service = TestResponseService::new(size);
                            let cached_service = layer.layer(service);

                            let request = Request::builder()
                                .uri(format!(
                                    "https://example.com/miss-test-{i}"
                                ))
                                .body(Full::new(Bytes::new()))
                                .unwrap();

                            let response =
                                cached_service.oneshot(request).await.unwrap();
                            let _body = black_box(
                                response.into_body().collect().await.unwrap(),
                            );
                        }

                        start.elapsed()
                    })
                })
            },
        );
    }

    group.finish();
}

// Benchmark cache hit performance - buffered vs streaming
fn bench_cache_hit_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_hit_comparison");
    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(8));

    let sizes = vec![
        ("1kb", 1024),
        ("10kb", 10 * 1024),
        ("100kb", 100 * 1024),
        ("1mb", 1024 * 1024),
        ("5mb", 5 * 1024 * 1024),
    ];

    for (size_name, size_bytes) in sizes {
        // Buffered benchmark
        group.bench_with_input(
            BenchmarkId::new("buffered", size_name),
            &size_bytes,
            |b, &size| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let temp_dir = tempfile::tempdir().unwrap();
                let cache_manager =
                    CACacheManager::new(temp_dir.path().to_path_buf(), false);
                let layer = HttpCacheLayer::new(cache_manager);
                let service = TestResponseService::new(size);
                let mut cached_service = layer.layer(service);

                // Prime the cache
                rt.block_on(async {
                    let prime_request = Request::builder()
                        .uri("https://example.com/hit-test")
                        .body(Full::new(Bytes::new()))
                        .unwrap();
                    let _prime_response =
                        cached_service.call(prime_request).await.unwrap();
                });

                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();

                        for _i in 0..iters {
                            let request = Request::builder()
                                .uri("https://example.com/hit-test")
                                .body(Full::new(Bytes::new()))
                                .unwrap();

                            let response =
                                cached_service.call(request).await.unwrap();
                            let _body = black_box(
                                response.into_body().collect().await.unwrap(),
                            );
                        }

                        start.elapsed()
                    })
                })
            },
        );

        // Streaming benchmark
        group.bench_with_input(
            BenchmarkId::new("streaming", size_name),
            &size_bytes,
            |b, &size| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let temp_dir = tempfile::tempdir().unwrap();
                let cache_manager =
                    FileCacheManager::new(temp_dir.path().to_path_buf());
                let layer = HttpCacheStreamingLayer::new(cache_manager);
                let service = TestResponseService::new(size);
                let mut cached_service = layer.layer(service);

                // Prime the cache
                rt.block_on(async {
                    let prime_request = Request::builder()
                        .uri("https://example.com/hit-test")
                        .body(Full::new(Bytes::new()))
                        .unwrap();
                    let _prime_response =
                        cached_service.call(prime_request).await.unwrap();
                });

                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();

                        for _i in 0..iters {
                            let request = Request::builder()
                                .uri("https://example.com/hit-test")
                                .body(Full::new(Bytes::new()))
                                .unwrap();

                            let response =
                                cached_service.call(request).await.unwrap();
                            let _body = black_box(
                                response.into_body().collect().await.unwrap(),
                            );
                        }

                        start.elapsed()
                    })
                })
            },
        );
    }

    group.finish();
}

// Benchmark streaming cache throughput with concurrent requests
fn bench_streaming_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_throughput");
    group.sample_size(20);
    group.measurement_time(std::time::Duration::from_secs(15));

    let concurrent_requests = vec![1, 5, 10, 20];

    for concurrent in concurrent_requests {
        group.bench_with_input(
            BenchmarkId::new("concurrent_hits", concurrent),
            &concurrent,
            |b, &concurrent| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let temp_dir = tempfile::tempdir().unwrap();
                let cache_manager = FileCacheManager::new(
                    temp_dir.path().to_path_buf(),
                );
                let layer = HttpCacheStreamingLayer::new(cache_manager);
                let service = TestResponseService::new(100 * 1024); // 100KB
                let mut cached_service = layer.layer(service);

                // Prime the cache
                rt.block_on(async {
                    let prime_request = Request::builder()
                        .uri("https://example.com/throughput-test")
                        .body(Full::new(Bytes::new()))
                        .unwrap();
                    let _prime_response = cached_service.call(prime_request).await.unwrap();
                });

                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();

                        for _i in 0..iters {
                            let mut handles = Vec::new();

                            for _j in 0..concurrent {
                                let mut service = cached_service.clone();
                                let handle = tokio::spawn(async move {
                                    let request = Request::builder()
                                        .uri("https://example.com/throughput-test")
                                        .body(Full::new(Bytes::new()))
                                        .unwrap();

                                    let response = service.call(request).await.unwrap();
                                    black_box(response.into_body().collect().await.unwrap())
                                });
                                handles.push(handle);
                            }

                            for handle in handles {
                                let _ = handle.await.unwrap();
                            }
                        }

                        start.elapsed()
                    })
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    cache_benchmarks,
    bench_cache_miss_comparison,
    bench_cache_hit_comparison,
    bench_streaming_throughput,
);

criterion_main!(cache_benchmarks);
