//! Basic HTTP caching with http-cache-tower-server and Axum
//!
//! This example runs a real HTTP server that you can test with curl:
//!
//! ```bash
//! # Start the server
//! cargo run --example axum_basic --features manager-cacache
//!
//! # Test caching behavior
//! curl -v http://localhost:3000/           # First request: MISS
//! curl -v http://localhost:3000/           # Second request: HIT
//! curl -v http://localhost:3000/users/42   # User endpoint with 30s cache
//! curl -v http://localhost:3000/no-cache   # Never cached
//! ```
//!
//! Run with: cargo run --example axum_basic --features manager-cacache

use axum::{
    error_handling::HandleErrorLayer,
    extract::Path,
    response::{IntoResponse, Response},
    routing::get,
    BoxError, Router,
};
use http::StatusCode;
use http_cache::CACacheManager;
use http_cache_tower_server::ServerCacheLayer;
use tempfile::TempDir;
use tower::ServiceBuilder;

#[tokio::main]
async fn main() {
    // Create cache storage (use a persistent path in production)
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let manager = CACacheManager::new(temp_dir.path().to_path_buf(), false);

    // Build the router with standard Axum handlers
    let app = Router::new()
        .route("/", get(index))
        .route("/users/{id}", get(get_user))
        .route("/no-cache", get(no_cache))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_cache_error))
                .layer(ServerCacheLayer::new(manager)),
        );

    // Run the server
    let listener =
        tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();

    println!("Server running at http://localhost:3000");
    println!();
    println!("Try these commands:");
    println!(
        "  curl -v http://localhost:3000/           # Watch X-Cache header"
    );
    println!(
        "  curl -v http://localhost:3000/users/42   # User-specific endpoint"
    );
    println!("  curl -v http://localhost:3000/no-cache   # Never cached");

    axum::serve(listener, app).await.unwrap();
}

async fn handle_cache_error(err: BoxError) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("Cache error: {}", err))
        .into_response()
}

async fn index() -> Response {
    (
        StatusCode::OK,
        [("cache-control", "max-age=60")],
        "Hello! This response is cached for 60 seconds.",
    )
        .into_response()
}

async fn get_user(Path(id): Path<u32>) -> Response {
    (
        StatusCode::OK,
        [("cache-control", "max-age=30")],
        format!("User {} - Cached for 30 seconds", id),
    )
        .into_response()
}

async fn no_cache() -> Response {
    (
        StatusCode::OK,
        [("cache-control", "no-store")],
        "This response is never cached",
    )
        .into_response()
}
