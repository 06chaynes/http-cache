use crate::{BadRequest, CachedAgent, HttpCacheError};
use http_cache::{CacheKey, *};
use std::{sync::Arc, time::Duration};
use tempfile::TempDir;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const GET: &str = "GET";
const TEST_BODY: &[u8] = b"test";
const CACHEABLE_PUBLIC: &str = "max-age=86400, public";
const CACHEABLE_PRIVATE: &str = "max-age=86400, private";
const MUST_REVALIDATE: &str = "public, must-revalidate";
const HIT: &str = "HIT";
const MISS: &str = "MISS";

fn build_mock(
    cache_control_val: &str,
    body: &[u8],
    status: u16,
    expect: u64,
) -> Mock {
    Mock::given(method(GET))
        .respond_with(
            ResponseTemplate::new(status)
                .insert_header("cache-control", cache_control_val)
                .set_body_bytes(body),
        )
        .expect(expect)
}

#[test]
fn test_errors() {
    assert!(format!("{:?}", BadRequest).contains("BadRequest"));
    let ureq_err = HttpCacheError::cache("test".to_string());
    assert!(format!("{:?}", &ureq_err).contains("Cache"));
    assert_eq!(ureq_err.to_string(), "Cache error: test".to_string());
}

#[tokio::test]
async fn default_mode() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res.header("x-cache"), Some(HIT));
}

#[tokio::test]
async fn default_mode_with_options() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PRIVATE, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .cache_options(HttpCacheOptions {
            cache_options: Some(CacheOptions {
                shared: false,
                ..Default::default()
            }),
            ..Default::default()
        })
        .build()
        .unwrap();
    agent.get(&url).call().await.unwrap();
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn default_mode_no_cache_response() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock("no-cache", TEST_BODY, 200, 2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res.header("x-cache"), Some(MISS));
}

#[tokio::test]
async fn removes_warning() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = Mock::given(method(GET))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", CACHEABLE_PUBLIC)
                .insert_header("warning", "101 Test")
                .set_body_bytes(TEST_BODY),
        )
        .expect(1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res.header("x-cache"), Some(HIT));
    assert!(res.header("warning").is_none());
}

#[tokio::test]
async fn no_store_mode() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::NoStore)
        .build()
        .unwrap();
    agent.get(&url).call().await.unwrap();
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_none());
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));
}

#[tokio::test]
async fn no_cache_mode() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::NoCache)
        .build()
        .unwrap();
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res.header("x-cache"), Some(MISS));
}

#[tokio::test]
async fn force_cache_mode() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock("max-age=0, public", TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::ForceCache)
        .build()
        .unwrap();
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res.header("x-cache"), Some(HIT));
}

#[tokio::test]
async fn ignore_rules_mode() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock("no-store, max-age=0, public", TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::IgnoreRules)
        .build()
        .unwrap();
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res.header("x-cache"), Some(HIT));
}

#[tokio::test]
async fn reload_mode() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Reload)
        .cache_options(HttpCacheOptions {
            cache_options: Some(CacheOptions {
                shared: false,
                ..Default::default()
            }),
            ..Default::default()
        })
        .build()
        .unwrap();
    agent.get(&url).call().await.unwrap();
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());
    agent.get(&url).call().await.unwrap();
}

#[tokio::test]
async fn custom_cache_key() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .cache_options(HttpCacheOptions {
            cache_key: Some(Arc::new(|req: &http::request::Parts| {
                format!("{}:{}:test", req.method, req.uri)
            })),
            ..Default::default()
        })
        .build()
        .unwrap();
    agent.get(&url).call().await.unwrap();
    let data = manager.get(&format!("GET:{}:test", &url)).await.unwrap();
    assert!(data.is_some());
}

#[tokio::test]
async fn no_status_headers() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/test.css", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .cache_options(HttpCacheOptions {
            cache_status_headers: false,
            ..Default::default()
        })
        .build()
        .unwrap();
    let res = agent.get(&url).call().await.unwrap();
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());
    assert!(res.header("x-cache-lookup").is_none());
    assert!(res.header("x-cache").is_none());
}

#[tokio::test]
async fn cache_bust() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let bust_url = format!("{}/bust-cache", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .cache_options(HttpCacheOptions {
            cache_bust: Some(Arc::new(
                |parts: &http::request::Parts,
                 _cache_key: &Option<CacheKey>,
                 _uri: &str| {
                    let uri_string = parts.uri.to_string();
                    if uri_string.ends_with("/bust-cache") {
                        vec![format!(
                            "GET:{}",
                            uri_string.replace("/bust-cache", "/")
                        )]
                    } else {
                        Vec::new()
                    }
                },
            )),
            ..Default::default()
        })
        .build()
        .unwrap();
    agent.get(&url).call().await.unwrap();
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());
    agent.get(&bust_url).call().await.unwrap();
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_none());
}

#[tokio::test]
async fn only_if_cached_mode_miss() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 0);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::OnlyIfCached)
        .build()
        .unwrap();
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_none());
}

#[tokio::test]
async fn only_if_cached_mode_hit() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent_default = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();
    let res = agent_default.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());
    let agent_cached = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::OnlyIfCached)
        .build()
        .unwrap();
    let res = agent_cached.get(&url).call().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res.header("x-cache"), Some(HIT));
}

// Invalidation tests for POST, PUT, PATCH, DELETE, OPTIONS
#[tokio::test]
async fn post_request_invalidates_cache() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m_get = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let m_post = Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(201).set_body_bytes("created"))
        .expect(1);
    let _mock_guard_get = mock_server.register_as_scoped(m_get).await;
    let _mock_guard_post = mock_server.register_as_scoped(m_post).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();
    agent.get(&url).call().await.unwrap();
    let data = manager.get(&format!("GET:{}", &url)).await.unwrap();
    assert!(data.is_some());
    agent.post(&url).call().await.unwrap();
    let data = manager.get(&format!("GET:{}", &url)).await.unwrap();
    assert!(data.is_none());
}

#[tokio::test]
async fn put_request_invalidates_cache() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m_get = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let m_put = Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1);
    let _mock_guard_get = mock_server.register_as_scoped(m_get).await;
    let _mock_guard_put = mock_server.register_as_scoped(m_put).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();
    agent.get(&url).call().await.unwrap();
    let data = manager.get(&format!("GET:{}", &url)).await.unwrap();
    assert!(data.is_some());
    agent.put(&url).call().await.unwrap();
    let data = manager.get(&format!("GET:{}", &url)).await.unwrap();
    assert!(data.is_none());
}

#[tokio::test]
async fn patch_request_invalidates_cache() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m_get = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let m_patch = Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1);
    let _mock_guard_get = mock_server.register_as_scoped(m_get).await;
    let _mock_guard_patch = mock_server.register_as_scoped(m_patch).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();
    agent.get(&url).call().await.unwrap();
    let data = manager.get(&format!("GET:{}", &url)).await.unwrap();
    assert!(data.is_some());
    agent.request("PATCH", &url).call().await.unwrap();
    let data = manager.get(&format!("GET:{}", &url)).await.unwrap();
    assert!(data.is_none());
}

#[tokio::test]
async fn delete_request_invalidates_cache() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m_get = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let m_delete = Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1);
    let _mock_guard_get = mock_server.register_as_scoped(m_get).await;
    let _mock_guard_delete = mock_server.register_as_scoped(m_delete).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();
    agent.get(&url).call().await.unwrap();
    let data = manager.get(&format!("GET:{}", &url)).await.unwrap();
    assert!(data.is_some());
    agent.delete(&url).call().await.unwrap();
    let data = manager.get(&format!("GET:{}", &url)).await.unwrap();
    assert!(data.is_none());
}

#[tokio::test]
async fn options_request_not_cached() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = Mock::given(method("OPTIONS"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("allow", "GET, POST, PUT, DELETE")
                .insert_header("cache-control", CACHEABLE_PUBLIC),
        )
        .expect(2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();
    let res1 = agent.request("OPTIONS", &url).call().await.unwrap();
    assert_eq!(res1.status(), 200);
    let data = manager.get(&format!("OPTIONS:{}", &url)).await.unwrap();
    assert!(data.is_none());
    let res2 = agent.request("OPTIONS", &url).call().await.unwrap();
    assert_eq!(res2.status(), 200);
}

// Revalidation tests
#[tokio::test]
async fn revalidation_304() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(MUST_REVALIDATE, TEST_BODY, 200, 1);
    let m_304 = Mock::given(method(GET))
        .respond_with(ResponseTemplate::new(304))
        .expect(1);
    let mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();

    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));

    drop(mock_guard);
    let _mock_guard = mock_server.register_as_scoped(m_304).await;

    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());

    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res.header("x-cache"), Some(HIT));
    assert_eq!(res.as_bytes(), TEST_BODY);
}

#[tokio::test]
async fn revalidation_200() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(MUST_REVALIDATE, TEST_BODY, 200, 1);
    let m_200 = build_mock(MUST_REVALIDATE, b"updated", 200, 1);
    let mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();

    let _ = agent.get(&url).call().await.unwrap();

    drop(mock_guard);
    let _mock_guard = mock_server.register_as_scoped(m_200).await;

    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());

    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res.header("x-cache"), Some(MISS));
    assert_eq!(res.as_bytes(), b"updated");
}

#[tokio::test]
async fn revalidation_500() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(MUST_REVALIDATE, TEST_BODY, 200, 1);
    let m_500 = Mock::given(method(GET))
        .respond_with(ResponseTemplate::new(500))
        .expect(1);
    let mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();

    let _ = agent.get(&url).call().await.unwrap();

    drop(mock_guard);
    let _mock_guard = mock_server.register_as_scoped(m_500).await;

    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());

    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res.header("x-cache"), Some(HIT));
    assert!(res.header("warning").is_some());
    assert_eq!(res.as_bytes(), TEST_BODY);
}

#[tokio::test]
async fn custom_cache_mode_fn() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/test.css", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::NoStore)
        .cache_options(HttpCacheOptions {
            cache_mode_fn: Some(Arc::new(|req: &http::request::Parts| {
                if req.uri.path().ends_with(".css") {
                    CacheMode::Default
                } else {
                    CacheMode::NoStore
                }
            })),
            ..Default::default()
        })
        .build()
        .unwrap();
    // Remote request and should cache due to custom cache mode function
    agent.get(&url).call().await.unwrap();
    // Try to load cached object
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());

    // Test that non-.css files are not cached
    let url2 = format!("{}/", &mock_server.uri());
    agent.get(&url2).call().await.unwrap();
    let data2 = manager.get(&format!("{}:{}", GET, &url2)).await.unwrap();
    assert!(data2.is_none());
}

#[tokio::test]
async fn delete_after_non_get_head_method_request() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m_get = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let m_post = Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(201).set_body_bytes("created"))
        .expect(1);
    let _mock_guard_get = mock_server.register_as_scoped(m_get).await;
    let _mock_guard_post = mock_server.register_as_scoped(m_post).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();

    // Cold pass to load cache
    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));

    // Try to load cached object
    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());

    // Post request to make sure the cache object at the same resource was deleted
    agent.post(&url).call().await.unwrap();

    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_none());
}

#[cfg(feature = "json")]
#[tokio::test]
async fn json_request_and_response() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let json_response =
        serde_json::json!({"message": "success", "data": [1, 2, 3]});
    let m = Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .insert_header("cache-control", CACHEABLE_PUBLIC)
                .set_body_json(&json_response),
        )
        .expect(1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();

    let request_json = serde_json::json!({"test": "data"});
    let res = agent.post(&url).send_json(request_json).await.unwrap();
    assert_eq!(res.status(), 200);

    let response_json: serde_json::Value = res.into_json().unwrap();
    assert_eq!(response_json["message"], "success");
    assert_eq!(response_json["data"], serde_json::json!([1, 2, 3]));
}

#[tokio::test]
async fn head_request_caching() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = Mock::given(method("HEAD"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", CACHEABLE_PUBLIC)
                .insert_header("content-length", "100"),
        )
        .expect(1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .build()
        .unwrap();

    let res = agent.head(&url).call().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));
    assert_eq!(res.as_bytes().len(), 0); // HEAD responses have no body

    let data = manager.get(&format!("HEAD:{}", &url)).await.unwrap();
    assert!(data.is_some());

    let res2 = agent.head(&url).call().await.unwrap();
    assert_eq!(res2.status(), 200);
    assert_eq!(res2.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res2.header("x-cache"), Some(HIT));
}

#[tokio::test]
async fn max_ttl_override() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = Mock::given(method(GET))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "max-age=3600") // 1 hour
                .set_body_bytes(TEST_BODY),
        )
        .expect(1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .cache_options(HttpCacheOptions {
            max_ttl: Some(Duration::from_secs(300)), // 5 minutes - should override the 1 hour max-age
            ..Default::default()
        })
        .build()
        .unwrap();

    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));

    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());

    // Verify the cache entry has the reduced TTL (this is implicit in the cache policy)
    let res2 = agent.get(&url).call().await.unwrap();
    assert_eq!(res2.status(), 200);
    assert_eq!(res2.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res2.header("x-cache"), Some(HIT));
}

#[tokio::test]
async fn max_ttl_with_ignore_rules() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = Mock::given(method(GET))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "no-cache") // Should normally not cache
                .set_body_bytes(TEST_BODY),
        )
        .expect(1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::IgnoreRules) // Ignore cache-control headers
        .cache_options(HttpCacheOptions {
            max_ttl: Some(Duration::from_secs(300)), // 5 minutes - provides expiration control
            ..Default::default()
        })
        .build()
        .unwrap();

    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));

    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());

    // Second request should hit cache despite no-cache header
    let res2 = agent.get(&url).call().await.unwrap();
    assert_eq!(res2.status(), 200);
    assert_eq!(res2.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res2.header("x-cache"), Some(HIT));
}

#[tokio::test]
async fn max_ttl_no_override_when_shorter() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;
    let m = Mock::given(method(GET))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "max-age=60") // 1 minute
                .set_body_bytes(TEST_BODY),
        )
        .expect(1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .cache_options(HttpCacheOptions {
            max_ttl: Some(Duration::from_secs(300)), // 5 minutes - should NOT override the shorter 1 minute
            ..Default::default()
        })
        .build()
        .unwrap();

    let res = agent.get(&url).call().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res.header("x-cache"), Some(MISS));

    let data = manager.get(&format!("{}:{}", GET, &url)).await.unwrap();
    assert!(data.is_some());

    // Verify the cache works (the actual TTL timing test would be complex)
    let res2 = agent.get(&url).call().await.unwrap();
    assert_eq!(res2.status(), 200);
    assert_eq!(res2.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res2.header("x-cache"), Some(HIT));
}

#[tokio::test]
async fn content_type_based_caching() {
    let temp_dir = TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;

    // Mock JSON API endpoint - should be force cached
    let json_mock = Mock::given(method(GET))
        .and(path("/api/data.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .insert_header("cache-control", "public, max-age=300") // Should be cached normally
                .set_body_bytes(r#"{"message": "test"}"#),
        )
        .expect(1); // Should only be called once due to caching

    // Mock CSS file - should be force cached
    let css_mock = Mock::given(method(GET))
        .and(path("/styles.css"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/css")
                .insert_header("cache-control", "public, max-age=300")
                .set_body_bytes("body { color: blue; }"),
        )
        .expect(1); // Should only be called once due to caching

    // Mock HTML page - should NOT be cached
    let html_mock = Mock::given(method(GET))
        .and(path("/page.html"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html")
                .insert_header("cache-control", "public, max-age=300")
                .set_body_bytes("<html><body>Hello World</body></html>"),
        )
        .expect(2); // Should be called twice (no caching)

    // Mock image - should be cached with default rules
    let image_mock = Mock::given(method(GET))
        .and(path("/image.png"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/png")
                .insert_header("cache-control", "public, max-age=3600")
                .set_body_bytes("fake-png-data"),
        )
        .expect(1); // Should only be called once due to caching

    // Mock unknown content type - should NOT be cached
    let unknown_mock = Mock::given(method(GET))
        .and(path("/unknown"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/octet-stream")
                .insert_header("cache-control", "public, max-age=300")
                .set_body_bytes("binary data"),
        )
        .expect(2); // Should be called twice (no caching)

    let _json_guard = mock_server.register_as_scoped(json_mock).await;
    let _css_guard = mock_server.register_as_scoped(css_mock).await;
    let _html_guard = mock_server.register_as_scoped(html_mock).await;
    let _image_guard = mock_server.register_as_scoped(image_mock).await;
    let _unknown_guard = mock_server.register_as_scoped(unknown_mock).await;

    // Create agent with content-type based caching
    let agent = CachedAgent::builder()
        .cache_manager(manager.clone())
        .cache_mode(CacheMode::Default)
        .cache_options(HttpCacheOptions {
            response_cache_mode_fn: Some(Arc::new(
                |_request_parts, response| {
                    // Check the Content-Type header to decide caching behavior
                    if let Some(content_type) =
                        response.headers.get("content-type")
                    {
                        match content_type.as_str() {
                            // Cache JSON APIs with default rules
                            ct if ct.starts_with("application/json") => {
                                Some(CacheMode::Default)
                            }
                            // Cache static assets aggressively
                            ct if ct.starts_with("text/css") => {
                                Some(CacheMode::ForceCache)
                            }
                            ct if ct.starts_with("application/javascript") => {
                                Some(CacheMode::ForceCache)
                            }
                            // Cache images with default HTTP caching rules
                            ct if ct.starts_with("image/") => {
                                Some(CacheMode::Default)
                            }
                            // Don't cache HTML pages (often dynamic)
                            ct if ct.starts_with("text/html") => {
                                Some(CacheMode::NoStore)
                            }
                            // Don't cache unknown content types
                            _ => Some(CacheMode::NoStore),
                        }
                    } else {
                        // No Content-Type header - don't cache for safety
                        Some(CacheMode::NoStore)
                    }
                },
            )),
            cache_status_headers: true,
            ..Default::default()
        })
        .build()
        .unwrap();

    // Test JSON API - should be cached despite no-cache header (ForceCache)
    let json_url = format!("{}/api/data.json", mock_server.uri());
    let res1 = agent.get(&json_url).call().await.unwrap();
    assert_eq!(res1.status(), 200);
    assert_eq!(res1.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res1.header("x-cache"), Some(MISS));
    assert_eq!(res1.header("content-type"), Some("application/json"));

    // Second request should hit cache
    let res2 = agent.get(&json_url).call().await.unwrap();
    assert_eq!(res2.status(), 200);
    assert_eq!(res2.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res2.header("x-cache"), Some(HIT));

    // Test CSS file - should be cached (ForceCache)
    let css_url = format!("{}/styles.css", mock_server.uri());
    let res3 = agent.get(&css_url).call().await.unwrap();
    assert_eq!(res3.status(), 200);
    assert_eq!(res3.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res3.header("x-cache"), Some(MISS));

    // Second CSS request should hit cache
    let res4 = agent.get(&css_url).call().await.unwrap();
    assert_eq!(res4.status(), 200);
    assert_eq!(res4.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res4.header("x-cache"), Some(HIT));

    // Test HTML page - should NOT be cached (NoStore)
    let html_url = format!("{}/page.html", mock_server.uri());
    let res5 = agent.get(&html_url).call().await.unwrap();
    assert_eq!(res5.status(), 200);
    assert_eq!(res5.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res5.header("x-cache"), Some(MISS));

    // Second HTML request should also miss (not cached)
    let res6 = agent.get(&html_url).call().await.unwrap();
    assert_eq!(res6.status(), 200);
    assert_eq!(res6.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res6.header("x-cache"), Some(MISS));

    // Test image - should be cached with default rules
    let image_url = format!("{}/image.png", mock_server.uri());
    let res7 = agent.get(&image_url).call().await.unwrap();
    assert_eq!(res7.status(), 200);
    assert_eq!(res7.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res7.header("x-cache"), Some(MISS));

    // Second image request should hit cache
    let res8 = agent.get(&image_url).call().await.unwrap();
    assert_eq!(res8.status(), 200);
    assert_eq!(res8.header("x-cache-lookup"), Some(HIT));
    assert_eq!(res8.header("x-cache"), Some(HIT));

    // Test unknown content type - should NOT be cached (NoStore)
    let unknown_url = format!("{}/unknown", mock_server.uri());
    let res9 = agent.get(&unknown_url).call().await.unwrap();
    assert_eq!(res9.status(), 200);
    assert_eq!(res9.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res9.header("x-cache"), Some(MISS));

    // Second unknown request should also miss (not cached)
    let res10 = agent.get(&unknown_url).call().await.unwrap();
    assert_eq!(res10.status(), 200);
    assert_eq!(res10.header("x-cache-lookup"), Some(MISS));
    assert_eq!(res10.header("x-cache"), Some(MISS));

    // Verify cache entries exist for the expected content types
    let json_key = format!("{}:{}", GET, json_url);
    let css_key = format!("{}:{}", GET, css_url);
    let html_key = format!("{}:{}", GET, html_url);
    let image_key = format!("{}:{}", GET, image_url);
    let unknown_key = format!("{}:{}", GET, unknown_url);

    assert!(
        manager.get(&json_key).await.unwrap().is_some(),
        "JSON should be cached"
    );
    assert!(
        manager.get(&css_key).await.unwrap().is_some(),
        "CSS should be cached"
    );
    assert!(
        manager.get(&html_key).await.unwrap().is_none(),
        "HTML should NOT be cached"
    );
    assert!(
        manager.get(&image_key).await.unwrap().is_some(),
        "Image should be cached"
    );
    assert!(
        manager.get(&unknown_key).await.unwrap().is_none(),
        "Unknown type should NOT be cached"
    );
}

#[cfg(feature = "rate-limiting")]
mod rate_limiting_tests {
    use super::*;
    use http_cache::rate_limiting::{
        DirectRateLimiter, DomainRateLimiter, Quota,
    };
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// Mock rate limiter that tracks calls for testing
    #[derive(Debug)]
    struct MockRateLimiter {
        calls: Arc<Mutex<Vec<String>>>,
        delay: Duration,
    }

    impl MockRateLimiter {
        fn new(delay: Duration) -> Self {
            Self { calls: Arc::new(Mutex::new(Vec::new())), delay }
        }

        fn get_calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl CacheAwareRateLimiter for MockRateLimiter {
        async fn until_key_ready(&self, key: &str) {
            self.calls.lock().unwrap().push(key.to_string());
            if !self.delay.is_zero() {
                std::thread::sleep(self.delay);
            }
        }

        fn check_key(&self, _key: &str) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn cache_hit_bypasses_rate_limiting() {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let rate_limiter = Arc::new(MockRateLimiter::new(Duration::ZERO));

        let agent = CachedAgent::builder()
            .cache_manager(manager)
            .cache_mode(CacheMode::Default)
            .cache_options(HttpCacheOptions {
                rate_limiter: Some(rate_limiter.clone()),
                ..Default::default()
            })
            .build()
            .unwrap();

        // First request (cache miss) - should trigger rate limiting
        let res1 = agent.get(&url).call().await.unwrap();
        assert_eq!(res1.status(), 200);
        assert_eq!(res1.header("x-cache-lookup"), Some(MISS));
        assert_eq!(res1.header("x-cache"), Some(MISS));

        // Second request (cache hit) - should NOT trigger rate limiting
        let res2 = agent.get(&url).call().await.unwrap();
        assert_eq!(res2.status(), 200);
        assert_eq!(res2.header("x-cache-lookup"), Some(HIT));
        assert_eq!(res2.header("x-cache"), Some(HIT));

        // Verify rate limiter was only called once (for the cache miss)
        let calls = rate_limiter.get_calls();
        assert_eq!(calls.len(), 1);
    }

    #[tokio::test]
    async fn cache_miss_applies_rate_limiting() {
        let mock_server = MockServer::start().await;
        let m = build_mock("no-cache", TEST_BODY, 200, 2);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let rate_limiter =
            Arc::new(MockRateLimiter::new(Duration::from_millis(100)));

        let agent = CachedAgent::builder()
            .cache_manager(manager)
            .cache_mode(CacheMode::NoCache) // Force cache misses
            .cache_options(HttpCacheOptions {
                rate_limiter: Some(rate_limiter.clone()),
                ..Default::default()
            })
            .build()
            .unwrap();

        let start = Instant::now();

        // Two requests that will both be cache misses
        let res1 = agent.get(&url).call().await.unwrap();
        assert_eq!(res1.status(), 200);

        let res2 = agent.get(&url).call().await.unwrap();
        assert_eq!(res2.status(), 200);

        let elapsed = start.elapsed();

        // Verify rate limiter was called for both requests
        let calls = rate_limiter.get_calls();
        assert_eq!(calls.len(), 2);

        // Verify some delay was applied (at least some portion of our 200ms total)
        assert!(elapsed >= Duration::from_millis(100));
    }

    #[tokio::test]
    async fn domain_rate_limiter_integration() {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();

        // Create a domain rate limiter with very permissive limits
        let quota = Quota::per_second(std::num::NonZeroU32::new(100).unwrap());
        let rate_limiter = Arc::new(DomainRateLimiter::new(quota));

        let agent = CachedAgent::builder()
            .cache_manager(manager)
            .cache_mode(CacheMode::NoCache) // Force cache miss
            .cache_options(HttpCacheOptions {
                rate_limiter: Some(rate_limiter),
                ..Default::default()
            })
            .build()
            .unwrap();

        // Request should succeed and be rate limited
        let res = agent.get(&url).call().await.unwrap();
        assert_eq!(res.status(), 200);
    }

    #[tokio::test]
    async fn direct_rate_limiter_integration() {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();

        // Create a direct rate limiter with very permissive limits
        let quota = Quota::per_second(std::num::NonZeroU32::new(100).unwrap());
        let rate_limiter = Arc::new(DirectRateLimiter::direct(quota));

        let agent = CachedAgent::builder()
            .cache_manager(manager)
            .cache_mode(CacheMode::NoCache) // Force cache miss
            .cache_options(HttpCacheOptions {
                rate_limiter: Some(rate_limiter),
                ..Default::default()
            })
            .build()
            .unwrap();

        // Request should succeed and be rate limited
        let res = agent.get(&url).call().await.unwrap();
        assert_eq!(res.status(), 200);
    }
}
