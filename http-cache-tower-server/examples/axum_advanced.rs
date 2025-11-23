//! Advanced HTTP caching with custom keyers, invalidation, and metrics
//!
//! This example demonstrates query-based caching, cache metrics, and invalidation.
//!
//! ## Quick Start
//!
//! ```bash
//! cargo run --example axum_advanced --features manager-cacache
//! ```
//!
//! ## Step-by-Step Demo
//!
//! ### 1. Check initial metrics (everything at zero)
//! ```bash
//! curl http://localhost:3000/metrics
//! # Cache Metrics:
//! #   Hits: 0
//! #   Misses: 0
//! #   Stores: 0
//! #   Hit Rate: 0.0%
//! ```
//!
//! ### 2. Make a search request (cache MISS)
//! ```bash
//! curl -i http://localhost:3000/search?q=rust
//! # HTTP/1.1 200 OK
//! # x-cache: MISS
//! # cache-control: public, max-age=300
//! # Search results for: rust
//! ```
//!
//! ### 3. Repeat the same request (cache HIT)
//! ```bash
//! curl -i http://localhost:3000/search?q=rust
//! # HTTP/1.1 200 OK
//! # x-cache: HIT
//! # Search results for: rust
//! ```
//!
//! ### 4. Try a different query (cache MISS - different cache key)
//! ```bash
//! curl -i http://localhost:3000/search?q=cache
//! # x-cache: MISS
//! ```
//!
//! ### 5. Check metrics again
//! ```bash
//! curl http://localhost:3000/metrics
//! # Cache Metrics:
//! #   Hits: 1
//! #   Misses: 2
//! #   Stores: 2
//! #   Hit Rate: 33.3%
//! ```
//!
//! ### 6. Invalidate a cached entry
//! ```bash
//! curl -X DELETE "http://localhost:3000/cache?key=GET%20/search?q=rust"
//! # Invalidated cache key: GET /search?q=rust
//! ```
//!
//! ### 7. Request again (cache MISS after invalidation)
//! ```bash
//! curl -i http://localhost:3000/search?q=rust
//! # x-cache: MISS
//! ```
//!
//! ## Other Endpoints
//!
//! ```bash
//! # Product details (cached 10 minutes)
//! curl http://localhost:3000/products/42
//!
//! # Dashboard (private cache - not stored in shared cache)
//! curl http://localhost:3000/dashboard
//! ```

use axum::{
    error_handling::HandleErrorLayer,
    extract::{Query, State},
    response::{IntoResponse, Response},
    routing::{delete, get},
    BoxError, Router,
};
use http::{Request, StatusCode};
use http_cache::CACacheManager;
use http_cache_tower_server::{
    CacheMetrics, CustomKeyer, QueryKeyer, ServerCacheLayer, ServerCacheOptions,
};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use tempfile::TempDir;
use tower::ServiceBuilder;

#[derive(Clone)]
struct AppState {
    metrics: Arc<CacheMetrics>,
    cache_layer: Arc<ServerCacheLayer<CACacheManager, QueryKeyer>>,
}

#[tokio::main]
async fn main() {
    // Create cache storage
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let manager = CACacheManager::new(temp_dir.path().to_path_buf(), false);

    // Configure cache options
    let options = ServerCacheOptions {
        default_ttl: Some(Duration::from_secs(120)),
        max_ttl: Some(Duration::from_secs(3600)),
        cache_status_headers: true,
        ..Default::default()
    };

    // Create cache layer with QueryKeyer (includes query params in cache key)
    let cache_layer =
        ServerCacheLayer::with_keyer(manager, QueryKeyer).with_options(options);

    // Store references for metrics and invalidation
    let state = AppState {
        metrics: cache_layer.metrics().clone(),
        cache_layer: Arc::new(cache_layer.clone()),
    };

    // Routes that should be cached
    let cached_routes = Router::new()
        .route("/search", get(search))
        .route("/dashboard", get(dashboard))
        .route("/products/{id}", get(get_product))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_cache_error))
                .layer(cache_layer),
        );

    // Monitoring routes bypass the cache
    let admin_routes = Router::new()
        .route("/metrics", get(metrics))
        .route("/cache", delete(invalidate_cache));

    // Merge all routes
    let app = Router::new()
        .merge(cached_routes)
        .merge(admin_routes)
        .with_state(state);

    // Run server
    let listener =
        tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();

    println!("Server running at http://localhost:3000");
    println!();
    println!("Endpoints:");
    println!("  GET  /search?q=...      - Cached by query params");
    println!("  GET  /dashboard         - User-specific content");
    println!("  GET  /products/:id      - Product details");
    println!("  GET  /metrics           - Cache statistics");
    println!("  DELETE /cache?key=...   - Invalidate cache entry");

    axum::serve(listener, app).await.unwrap();
}

async fn handle_cache_error(err: BoxError) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("Cache error: {}", err))
        .into_response()
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(Query(params): Query<SearchQuery>) -> Response {
    // Simulate database query
    tokio::time::sleep(Duration::from_millis(50)).await;

    (
        StatusCode::OK,
        [("cache-control", "public, max-age=300")],
        format!("Search results for: {}", params.q),
    )
        .into_response()
}

async fn dashboard() -> Response {
    // Note: In a real app, you'd use a CustomKeyer that includes session ID
    // to prevent serving User A's dashboard to User B
    (
        StatusCode::OK,
        [("cache-control", "private, max-age=60")],
        "User dashboard - private cache only",
    )
        .into_response()
}

async fn get_product(
    axum::extract::Path(id): axum::extract::Path<u32>,
) -> Response {
    // Simulate slow database lookup
    tokio::time::sleep(Duration::from_millis(100)).await;

    (
        StatusCode::OK,
        [("cache-control", "public, max-age=600")],
        format!("Product {} details - cached for 10 minutes", id),
    )
        .into_response()
}

async fn metrics(State(state): State<AppState>) -> Response {
    let metrics = &state.metrics;
    let hits = metrics.hits.load(std::sync::atomic::Ordering::Relaxed);
    let misses = metrics.misses.load(std::sync::atomic::Ordering::Relaxed);
    let stores = metrics.stores.load(std::sync::atomic::Ordering::Relaxed);

    let total = hits + misses;
    let hit_rate =
        if total > 0 { (hits as f64 / total as f64) * 100.0 } else { 0.0 };

    let body = format!(
        "Cache Metrics:\n  Hits: {}\n  Misses: {}\n  Stores: {}\n  Hit Rate: {:.1}%",
        hits, misses, stores, hit_rate
    );

    (StatusCode::OK, [("cache-control", "no-store")], body).into_response()
}

#[derive(Deserialize)]
struct InvalidateQuery {
    key: String,
}

async fn invalidate_cache(
    State(state): State<AppState>,
    Query(params): Query<InvalidateQuery>,
) -> Response {
    match state.cache_layer.invalidate(&params.key).await {
        Ok(()) => {
            (StatusCode::OK, format!("Invalidated cache key: {}", params.key))
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to invalidate: {}", e),
        )
            .into_response(),
    }
}

// Example: Creating a session-aware cache layer
#[allow(dead_code)]
fn create_session_cache_layer(
    manager: CACacheManager,
) -> ServerCacheLayer<
    CACacheManager,
    CustomKeyer<impl Fn(&Request<()>) -> String + Clone>,
> {
    let keyer = CustomKeyer::new(|req: &Request<()>| {
        let session = req
            .headers()
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|cookies| {
                cookies
                    .split(';')
                    .find_map(|c| c.trim().strip_prefix("session="))
            })
            .unwrap_or("anonymous");

        format!("{} {} session:{}", req.method(), req.uri().path(), session)
    });

    ServerCacheLayer::with_keyer(manager, keyer)
}
