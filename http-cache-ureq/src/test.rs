use crate::{error, CachedAgent};
use http_cache::{CacheKey, *};
use macro_rules_attribute::apply;
use smol_macros::test;
use std::sync::Arc;
use tempfile::TempDir;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

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
    assert!(format!("{:?}", error::BadRequest).contains("BadRequest"));
    let ureq_err = error::UreqError::Cache("test".to_string());
    assert!(format!("{:?}", &ureq_err).contains("Cache"));
    assert_eq!(ureq_err.to_string(), "Cache error: test".to_string());
}

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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
#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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
#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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

#[apply(test!)]
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
#[apply(test!)]
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

#[apply(test!)]
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
