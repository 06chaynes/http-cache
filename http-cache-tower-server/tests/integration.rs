use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use http_cache::{CacheManager, HttpResponse, Result};
use http_cache_semantics::CachePolicy;
use http_cache_tower_server::{
    CustomKeyer, DefaultKeyer, Keyer, QueryKeyer, ServerCacheLayer,
    ServerCacheOptions,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower::{Layer, Service, ServiceExt};

// Extension type for testing path parameter preservation
#[derive(Debug, Clone, PartialEq)]
struct PathParams {
    id: String,
}

// Simple in-memory cache manager for testing
#[derive(Clone)]
struct MemoryCacheManager {
    store: Arc<Mutex<HashMap<String, (HttpResponse, CachePolicy)>>>,
}

impl MemoryCacheManager {
    fn new() -> Self {
        Self { store: Arc::new(Mutex::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl CacheManager for MemoryCacheManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        Ok(self.store.lock().unwrap().get(cache_key).cloned())
    }

    async fn put(
        &self,
        cache_key: String,
        res: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        self.store.lock().unwrap().insert(cache_key, (res.clone(), policy));
        Ok(res)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        self.store.lock().unwrap().remove(cache_key);
        Ok(())
    }
}

#[tokio::test]
async fn test_cache_hit_and_miss() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=60")
                    .body(Full::new(Bytes::from("Hello, World!")))
                    .unwrap(),
            )
        }));

    // First request - cache miss
    let req = Request::get("/test").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    // Give cache write time to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request - cache hit
    let req = Request::get("/test").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "HIT");
}

#[tokio::test]
async fn test_no_store_directive() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "no-store")
                    .body(Full::new(Bytes::from("Don't cache me")))
                    .unwrap(),
            )
        }));

    // Request should not be cached
    let req = Request::get("/no-store").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();

    // Should not have cache header if not cached
    assert!(
        res.headers().get("x-cache").is_none()
            || res.headers().get("x-cache").unwrap() != "MISS"
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request should also not hit cache
    let req = Request::get("/no-store").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();

    // Should not be a cache hit
    assert!(
        res.headers().get("x-cache").is_none()
            || res.headers().get("x-cache").unwrap() != "HIT"
    );
}

#[tokio::test]
async fn test_private_directive() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "private, max-age=60")
                    .body(Full::new(Bytes::from("Private data")))
                    .unwrap(),
            )
        }));

    // Request should not be cached (shared cache)
    let req = Request::get("/private").body(Full::new(Bytes::new())).unwrap();
    let _res = service.ready().await.unwrap().call(req).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request should not hit cache
    let req = Request::get("/private").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert!(
        res.headers().get("x-cache").is_none()
            || res.headers().get("x-cache").unwrap() != "HIT"
    );
}

#[tokio::test]
async fn test_s_maxage_override() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=60, s-maxage=120")
                    .body(Full::new(Bytes::from("Shared cache data")))
                    .unwrap(),
            )
        }));

    // Request should be cached with s-maxage (120s, not 60s)
    let req = Request::get("/s-maxage").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request should hit cache
    let req = Request::get("/s-maxage").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "HIT");
}

#[tokio::test]
async fn test_only_cache_success_status() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header("cache-control", "max-age=60")
                    .body(Full::new(Bytes::from("Not found")))
                    .unwrap(),
            )
        }));

    // 404 should not be cached
    let req = Request::get("/not-found").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request should not hit cache
    let req = Request::get("/not-found").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert!(
        res.headers().get("x-cache").is_none()
            || res.headers().get("x-cache").unwrap() != "HIT"
    );
}

#[tokio::test]
async fn test_default_keyer() {
    let keyer = DefaultKeyer;
    let req = Request::get("/users/123?page=1").body(()).unwrap();
    let key = keyer.cache_key(&req);

    // Default keyer should only include path, not query
    assert_eq!(key, "GET /users/123");
}

#[tokio::test]
async fn test_query_keyer() {
    let keyer = QueryKeyer;
    let req = Request::get("/users/123?page=1").body(()).unwrap();
    let key = keyer.cache_key(&req);

    // Query keyer should include query parameters
    assert_eq!(key, "GET /users/123?page=1");
}

#[tokio::test]
async fn test_body_size_limit() {
    let manager = MemoryCacheManager::new();
    let options = ServerCacheOptions {
        max_body_size: 10, // Very small limit
        ..Default::default()
    };
    let layer = ServerCacheLayer::new(manager.clone()).with_options(options);

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=60")
                    .body(Full::new(Bytes::from(
                        "This is a long response body",
                    )))
                    .unwrap(),
            )
        }));

    // First request - too large to cache
    let req = Request::get("/large").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request - should not hit cache
    let req = Request::get("/large").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");
}

#[tokio::test]
async fn test_ttl_constraints() {
    let manager = MemoryCacheManager::new();
    let options = ServerCacheOptions {
        min_ttl: Some(std::time::Duration::from_secs(30)),
        max_ttl: Some(std::time::Duration::from_secs(90)),
        ..Default::default()
    };
    let layer = ServerCacheLayer::new(manager.clone()).with_options(options);

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=10") // Below min_ttl
                    .body(Full::new(Bytes::from("Response")))
                    .unwrap(),
            )
        }));

    // Request should be cached with min_ttl (30s, not 10s)
    let req = Request::get("/ttl").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request should hit cache
    let req = Request::get("/ttl").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "HIT");
}

#[tokio::test]
async fn test_public_directive() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "public")
                    .body(Full::new(Bytes::from("Public data")))
                    .unwrap(),
            )
        }));

    // Request should be cached with default TTL
    let req = Request::get("/public").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request should hit cache
    let req = Request::get("/public").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "HIT");
}

#[tokio::test]
async fn test_no_cache_directive() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "no-cache")
                    .body(Full::new(Bytes::from("No cache")))
                    .unwrap(),
            )
        }));

    // Request should not be cached
    let req = Request::get("/no-cache").body(Full::new(Bytes::new())).unwrap();
    let _res = service.ready().await.unwrap().call(req).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request should not hit cache
    let req = Request::get("/no-cache").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert!(
        res.headers().get("x-cache").is_none()
            || res.headers().get("x-cache").unwrap() != "HIT"
    );
}

#[tokio::test]
async fn test_expires_future_date() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let future_time =
        std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    let expires_date = httpdate::fmt_http_date(future_time);

    let mut service =
        layer.layer(tower::service_fn(move |_req: Request<Full<Bytes>>| {
            let expires = expires_date.clone();
            async move {
                Ok::<_, std::io::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("expires", expires)
                        .body(Full::new(Bytes::from("Cacheable with Expires")))
                        .unwrap(),
                )
            }
        }));

    let req =
        Request::get("/expires-future").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req =
        Request::get("/expires-future").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "HIT");
}

#[tokio::test]
async fn test_expires_past_date() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let past_time =
        std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    let expires_date = httpdate::fmt_http_date(past_time);

    let mut service =
        layer.layer(tower::service_fn(move |_req: Request<Full<Bytes>>| {
            let expires = expires_date.clone();
            async move {
                Ok::<_, std::io::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("expires", expires)
                        .body(Full::new(Bytes::from("Already expired")))
                        .unwrap(),
                )
            }
        }));

    let req =
        Request::get("/expires-past").body(Full::new(Bytes::new())).unwrap();
    let _res = service.ready().await.unwrap().call(req).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req =
        Request::get("/expires-past").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert!(
        res.headers().get("x-cache").is_none()
            || res.headers().get("x-cache").unwrap() != "HIT"
    );
}

#[tokio::test]
async fn test_expires_invalid_format() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("expires", "not-a-valid-date")
                    .body(Full::new(Bytes::from("Invalid expires")))
                    .unwrap(),
            )
        }));

    let req =
        Request::get("/invalid-expires").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req =
        Request::get("/invalid-expires").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert!(
        res.headers().get("x-cache").is_none()
            || res.headers().get("x-cache").unwrap() != "HIT"
    );
}

#[tokio::test]
async fn test_cache_control_overrides_expires() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let future_time =
        std::time::SystemTime::now() + std::time::Duration::from_secs(10);
    let expires_date = httpdate::fmt_http_date(future_time);

    let mut service =
        layer.layer(tower::service_fn(move |_req: Request<Full<Bytes>>| {
            let expires = expires_date.clone();
            async move {
                Ok::<_, std::io::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("cache-control", "max-age=60")
                        .header("expires", expires)
                        .body(Full::new(Bytes::from("Both headers")))
                        .unwrap(),
                )
            }
        }));

    let req =
        Request::get("/both-headers").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req =
        Request::get("/both-headers").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "HIT");
}

#[tokio::test]
async fn test_expires_only_no_cache_control() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let future_time =
        std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    let expires_date = httpdate::fmt_http_date(future_time);

    let mut service =
        layer.layer(tower::service_fn(move |_req: Request<Full<Bytes>>| {
            let expires = expires_date.clone();
            async move {
                Ok::<_, std::io::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("expires", expires)
                        .body(Full::new(Bytes::from("Expires only")))
                        .unwrap(),
                )
            }
        }));

    let req =
        Request::get("/expires-only").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req =
        Request::get("/expires-only").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "HIT");
}

#[tokio::test]
async fn test_expires_with_ttl_constraints() {
    let manager = MemoryCacheManager::new();
    let options = ServerCacheOptions {
        max_ttl: Some(std::time::Duration::from_secs(30)),
        ..Default::default()
    };
    let layer = ServerCacheLayer::new(manager.clone()).with_options(options);

    let future_time =
        std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
    let expires_date = httpdate::fmt_http_date(future_time);

    let mut service =
        layer.layer(tower::service_fn(move |_req: Request<Full<Bytes>>| {
            let expires = expires_date.clone();
            async move {
                Ok::<_, std::io::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("expires", expires)
                        .body(Full::new(Bytes::from(
                            "Long expires with max_ttl",
                        )))
                        .unwrap(),
                )
            }
        }));

    let req =
        Request::get("/expires-capped").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req =
        Request::get("/expires-capped").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "HIT");
}

#[tokio::test]
async fn test_concurrent_cache_requests() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let request_count = Arc::new(Mutex::new(0));
    let request_count_clone = request_count.clone();

    let mut service =
        layer.layer(tower::service_fn(move |_req: Request<Full<Bytes>>| {
            let count = request_count_clone.clone();
            async move {
                // Increment counter to track how many times backend is called
                *count.lock().unwrap() += 1;
                tokio::time::sleep(tokio::time::Duration::from_millis(50))
                    .await;
                Ok::<_, std::io::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("cache-control", "max-age=60")
                        .body(Full::new(Bytes::from("Concurrent response")))
                        .unwrap(),
                )
            }
        }));

    // Make multiple concurrent requests to the same endpoint
    let mut handles = vec![];
    for _ in 0..5 {
        let req =
            Request::get("/concurrent").body(Full::new(Bytes::new())).unwrap();
        let mut svc = service.clone();
        let handle = tokio::spawn(async move {
            svc.ready().await.unwrap().call(req).await.unwrap()
        });
        handles.push(handle);
    }

    // Wait for all requests to complete
    let mut responses = vec![];
    for handle in handles {
        responses.push(handle.await.unwrap());
    }

    // Verify all requests succeeded
    assert_eq!(responses.len(), 5, "All concurrent requests should complete");

    // At least one should be a MISS (the first one)
    let miss_count = responses
        .iter()
        .filter(|r| {
            r.headers().get("x-cache").map(|v| v == "MISS").unwrap_or(false)
        })
        .count();
    assert!(miss_count >= 1, "At least one request should be a cache MISS");

    // Give cache writes time to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Verify subsequent request hits cache
    let req =
        Request::get("/concurrent").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "HIT",
        "Subsequent request should hit cache"
    );
}

#[tokio::test]
async fn test_stale_cache_expiration() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=1") // Very short TTL
                    .body(Full::new(Bytes::from("Expires soon")))
                    .unwrap(),
            )
        }));

    // First request - cache miss
    let req = Request::get("/stale").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "MISS",
        "First request should be a cache MISS"
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request - should hit cache while still fresh
    let req = Request::get("/stale").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "HIT",
        "Request within TTL should be a cache HIT"
    );

    // Wait for cache entry to expire (1 second + buffer)
    tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

    // Third request - should be a miss due to expiration
    let req = Request::get("/stale").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "MISS",
        "Request after expiration should be a cache MISS"
    );
}

#[tokio::test]
async fn test_multiple_directives() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header(
                        "cache-control",
                        "max-age=60, public, must-revalidate",
                    )
                    .body(Full::new(Bytes::from("Multiple directives")))
                    .unwrap(),
            )
        }));

    // First request - cache miss
    let req =
        Request::get("/multi-directive").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "MISS",
        "First request should be a cache MISS"
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request - should hit cache (all directives should be recognized)
    let req =
        Request::get("/multi-directive").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "HIT",
        "Cache should handle multiple directives correctly"
    );

    // Also test with spaces variations
    let mut service2 =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=60,public,s-maxage=120")
                    .body(Full::new(Bytes::from("No spaces")))
                    .unwrap(),
            )
        }));

    let req =
        Request::get("/multi-no-space").body(Full::new(Bytes::new())).unwrap();
    let res = service2.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "MISS",
        "Should handle directives without spaces"
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req =
        Request::get("/multi-no-space").body(Full::new(Bytes::new())).unwrap();
    let res = service2.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "HIT",
        "Should cache with directives without spaces"
    );
}

#[tokio::test]
async fn test_malformed_cache_control() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    // Test with invalid directive
    let mut service1 = layer.clone().layer(tower::service_fn(
        |_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "invalid-directive")
                    .body(Full::new(Bytes::from("Invalid directive")))
                    .unwrap(),
            )
        },
    ));

    let req = Request::get("/invalid-directive")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let res = service1.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "Should handle invalid directive gracefully"
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req = Request::get("/invalid-directive")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let res = service1.ready().await.unwrap().call(req).await.unwrap();
    // Should not cache with invalid directive
    assert!(
        res.headers().get("x-cache").is_none()
            || res.headers().get("x-cache").unwrap() != "HIT",
        "Should not cache with invalid directive"
    );

    // Test with malformed max-age value
    let mut service2 = layer.clone().layer(tower::service_fn(
        |_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=notanumber")
                    .body(Full::new(Bytes::from("Invalid max-age")))
                    .unwrap(),
            )
        },
    ));

    let req =
        Request::get("/bad-max-age").body(Full::new(Bytes::new())).unwrap();
    let res = service2.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "Should handle malformed max-age gracefully"
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req =
        Request::get("/bad-max-age").body(Full::new(Bytes::new())).unwrap();
    let res = service2.ready().await.unwrap().call(req).await.unwrap();
    // Should not cache with malformed max-age
    assert!(
        res.headers().get("x-cache").is_none()
            || res.headers().get("x-cache").unwrap() != "HIT",
        "Should not cache with malformed max-age"
    );

    // Test with empty cache-control
    let mut service3 =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "")
                    .body(Full::new(Bytes::from("Empty cache-control")))
                    .unwrap(),
            )
        }));

    let req = Request::get("/empty-cc").body(Full::new(Bytes::new())).unwrap();
    let res = service3.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "Should handle empty cache-control gracefully"
    );
}

#[tokio::test]
async fn test_path_parameter_preservation() {
    // This is a regression test for issue #121
    // Verifies that request extensions (like Axum path parameters) are preserved
    // through the caching layer and accessible to the handler

    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    // Counter to track handler invocations
    let call_count = Arc::new(Mutex::new(0));
    let call_count_clone = call_count.clone();

    let mut service =
        layer.layer(tower::service_fn(move |req: Request<Full<Bytes>>| {
            let count = call_count_clone.clone();
            async move {
                // Increment handler call counter
                *count.lock().unwrap() += 1;

                // Extract the path parameter from request extensions
                // This simulates what Axum does after routing
                let path_params = req
                    .extensions()
                    .get::<PathParams>()
                    .expect("PathParams extension should be present");

                // Generate response that includes the path parameter
                // This proves the extension was preserved through the cache layer
                let body = format!("User ID: {}", path_params.id);

                Ok::<_, std::io::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("cache-control", "max-age=60")
                        .body(Full::new(Bytes::from(body)))
                        .unwrap(),
                )
            }
        }));

    // First request with path parameter "123" - should be a cache miss
    let mut req1 =
        Request::get("/users/123").body(Full::new(Bytes::new())).unwrap();
    req1.extensions_mut().insert(PathParams { id: "123".to_string() });

    let res1 = service.ready().await.unwrap().call(req1).await.unwrap();
    assert_eq!(res1.status(), StatusCode::OK);
    assert_eq!(res1.headers().get("x-cache").unwrap(), "MISS");

    // Collect body to verify the handler received the correct extension
    let body1 = http_body_util::BodyExt::collect(res1.into_body())
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(
        body1, "User ID: 123",
        "Handler should receive path parameter on cache miss"
    );

    // Verify handler was called once
    assert_eq!(
        *call_count.lock().unwrap(),
        1,
        "Handler should be called on cache miss"
    );

    // Give cache write time to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request to same path with same parameter - should be a cache hit
    let mut req2 =
        Request::get("/users/123").body(Full::new(Bytes::new())).unwrap();
    req2.extensions_mut().insert(PathParams { id: "123".to_string() });

    let res2 = service.ready().await.unwrap().call(req2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    assert_eq!(res2.headers().get("x-cache").unwrap(), "HIT");

    // Verify cached response has correct content
    let body2 = http_body_util::BodyExt::collect(res2.into_body())
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(
        body2, "User ID: 123",
        "Cached response should have correct content"
    );

    // Verify handler was NOT called again (cache hit)
    assert_eq!(
        *call_count.lock().unwrap(),
        1,
        "Handler should not be called on cache hit"
    );

    // Third request with different path parameter - should be a cache miss
    // This verifies that different requests don't interfere with each other
    let mut req3 =
        Request::get("/users/456").body(Full::new(Bytes::new())).unwrap();
    req3.extensions_mut().insert(PathParams { id: "456".to_string() });

    let res3 = service.ready().await.unwrap().call(req3).await.unwrap();
    assert_eq!(res3.status(), StatusCode::OK);
    assert_eq!(res3.headers().get("x-cache").unwrap(), "MISS");

    // Verify the handler received the NEW path parameter
    let body3 = http_body_util::BodyExt::collect(res3.into_body())
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(
        body3, "User ID: 456",
        "Handler should receive different path parameter for different request"
    );

    // Verify handler was called again for the new path
    assert_eq!(
        *call_count.lock().unwrap(),
        2,
        "Handler should be called for new path"
    );
}

#[tokio::test]
async fn test_request_extensions_not_stripped() {
    // Verifies that the cache layer doesn't strip request extensions
    // even when they're not used by the handler

    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    // Custom extension type
    #[derive(Debug, Clone, PartialEq)]
    struct CustomExtension {
        value: String,
    }

    let mut service = layer.layer(tower::service_fn(
        |req: Request<Full<Bytes>>| async move {
            // Verify extension is still present
            let ext = req.extensions().get::<CustomExtension>();
            assert!(
                ext.is_some(),
                "Extension should be preserved through cache layer"
            );
            assert_eq!(ext.unwrap().value, "test-value");

            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=60")
                    .body(Full::new(Bytes::from("OK")))
                    .unwrap(),
            )
        },
    ));

    // Make request with custom extension
    let mut req = Request::get("/test").body(Full::new(Bytes::new())).unwrap();
    req.extensions_mut()
        .insert(CustomExtension { value: "test-value".to_string() });

    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");
}

#[tokio::test]
async fn test_cache_by_default_option() {
    let manager = MemoryCacheManager::new();

    // With cache_by_default = false (default), no caching without directives
    let options_disabled =
        ServerCacheOptions { cache_by_default: false, ..Default::default() };
    let layer =
        ServerCacheLayer::new(manager.clone()).with_options(options_disabled);

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(Bytes::from("No directives")))
                    .unwrap(),
            )
        }));

    let req =
        Request::get("/no-directive").body(Full::new(Bytes::new())).unwrap();
    let _res = service.ready().await.unwrap().call(req).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req =
        Request::get("/no-directive").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert!(
        res.headers().get("x-cache").is_none()
            || res.headers().get("x-cache").unwrap() != "HIT",
        "Should not cache without directives when cache_by_default is false"
    );

    // With cache_by_default = true, should cache even without directives
    let options_enabled =
        ServerCacheOptions { cache_by_default: true, ..Default::default() };
    let layer =
        ServerCacheLayer::new(manager.clone()).with_options(options_enabled);

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(Bytes::from("No directives but cached")))
                    .unwrap(),
            )
        }));

    let req = Request::get("/cache-by-default")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req = Request::get("/cache-by-default")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "HIT",
        "Should cache without directives when cache_by_default is true"
    );
}

#[tokio::test]
async fn test_custom_keyer() {
    let manager = MemoryCacheManager::new();

    // Create a custom keyer that includes a header in the cache key
    let keyer = CustomKeyer::new(|req: &Request<()>| {
        let lang = req
            .headers()
            .get("accept-language")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("en");
        format!("{} {} lang:{}", req.method(), req.uri().path(), lang)
    });

    let layer = ServerCacheLayer::with_keyer(manager.clone(), keyer);

    let mut service = layer.layer(tower::service_fn(
        |req: Request<Full<Bytes>>| async move {
            let lang = req
                .headers()
                .get("accept-language")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("en");
            let body = format!("Response for {}", lang);
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=60")
                    .body(Full::new(Bytes::from(body)))
                    .unwrap(),
            )
        },
    ));

    // Request with English
    let req = Request::get("/test")
        .header("accept-language", "en")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Same request with English - should hit cache
    let req = Request::get("/test")
        .header("accept-language", "en")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "HIT");

    // Request with French - should miss cache (different key)
    let req = Request::get("/test")
        .header("accept-language", "fr")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "MISS",
        "Different language should have different cache key"
    );
}

#[tokio::test]
async fn test_directive_parsing_edge_cases() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    // Test that "no-store-something" does NOT match "no-store"
    let mut service1 = layer.clone().layer(tower::service_fn(
        |_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    // This should NOT prevent caching since it's not "no-store"
                    .header("cache-control", "max-age=60, no-store-custom")
                    .body(Full::new(Bytes::from("Should be cached")))
                    .unwrap(),
            )
        },
    ));

    let req =
        Request::get("/no-store-custom").body(Full::new(Bytes::new())).unwrap();
    let res = service1.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req =
        Request::get("/no-store-custom").body(Full::new(Bytes::new())).unwrap();
    let res = service1.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "HIT",
        "no-store-custom should not prevent caching"
    );

    // Test that "private-something" does NOT match "private"
    let mut service2 = layer.clone().layer(tower::service_fn(
        |_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=60, private-ext")
                    .body(Full::new(Bytes::from("Should be cached")))
                    .unwrap(),
            )
        },
    ));

    let req =
        Request::get("/private-ext").body(Full::new(Bytes::new())).unwrap();
    let res = service2.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let req =
        Request::get("/private-ext").body(Full::new(Bytes::new())).unwrap();
    let res = service2.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "HIT",
        "private-ext should not prevent caching"
    );
}

#[tokio::test]
async fn test_zero_max_age() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=0")
                    .body(Full::new(Bytes::from("Zero TTL")))
                    .unwrap(),
            )
        }));

    // First request
    let req = Request::get("/zero-ttl").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request - should not hit cache because TTL is 0
    let req = Request::get("/zero-ttl").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    // With zero TTL, entry is immediately stale
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "MISS",
        "Zero max-age should result in immediately stale cache entry"
    );
}

#[tokio::test]
async fn test_different_http_methods() {
    let manager = MemoryCacheManager::new();
    let layer = ServerCacheLayer::new(manager.clone());

    let mut service = layer.layer(tower::service_fn(
        |req: Request<Full<Bytes>>| async move {
            let body = format!("Method: {}", req.method());
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=60")
                    .body(Full::new(Bytes::from(body)))
                    .unwrap(),
            )
        },
    ));

    // GET request
    let req =
        Request::get("/method-test").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "MISS");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Same GET request - should hit cache
    let req =
        Request::get("/method-test").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(res.headers().get("x-cache").unwrap(), "HIT");

    // POST request to same path - should be a different cache key
    let req =
        Request::post("/method-test").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert_eq!(
        res.headers().get("x-cache").unwrap(),
        "MISS",
        "POST should have different cache key than GET"
    );
}

#[tokio::test]
async fn test_cache_status_headers_disabled() {
    let manager = MemoryCacheManager::new();
    let options = ServerCacheOptions {
        cache_status_headers: false,
        ..Default::default()
    };
    let layer = ServerCacheLayer::new(manager.clone()).with_options(options);

    let mut service =
        layer.layer(tower::service_fn(|_req: Request<Full<Bytes>>| async {
            Ok::<_, std::io::Error>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "max-age=60")
                    .body(Full::new(Bytes::from("No status headers")))
                    .unwrap(),
            )
        }));

    // First request
    let req = Request::get("/no-status").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert!(
        res.headers().get("x-cache").is_none(),
        "Should not have x-cache header when disabled"
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Second request - should hit cache but no header
    let req = Request::get("/no-status").body(Full::new(Bytes::new())).unwrap();
    let res = service.ready().await.unwrap().call(req).await.unwrap();
    assert!(
        res.headers().get("x-cache").is_none(),
        "Should not have x-cache header when disabled, even on HIT"
    );
}
