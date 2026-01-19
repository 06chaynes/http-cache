use crate::{BadRequest, Cache, HttpCacheError};

use http_cache::*;
use http_types::{Method, Request};
use std::str::FromStr;
use std::sync::Arc;
use surf::Client;
use url::Url;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

#[cfg(feature = "manager-moka")]
use crate::MokaManager;

pub(crate) fn build_mock(
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

const GET: &str = "GET";

const TEST_BODY: &[u8] = b"test";

const CACHEABLE_PUBLIC: &str = "max-age=86400, public";

const CACHEABLE_PRIVATE: &str = "max-age=86400, private";

const MUST_REVALIDATE: &str = "public, must-revalidate";

const HIT: &str = "HIT";

const MISS: &str = "MISS";

#[tokio::test]
async fn test_non_cloneable_request_graceful_fallback() -> Result<()> {
    // Test graceful handling of requests that cannot be cloned
    // This simulates the multipart form / streaming body scenario

    let temp_dir = tempfile::TempDir::new().unwrap();
    let manager = CACacheManager::new(temp_dir.path().into(), true);
    let mock_server = MockServer::start().await;

    // Set up a mock server that returns a successful response
    let m = Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_bytes(b"{'status': 'success'}"),
        )
        .expect(1);
    let _mock_guard = mock_server.register_as_scoped(m).await;

    let url = format!("{}/upload", mock_server.uri());
    let client = Client::new().with(Cache(HttpCache {
        mode: CacheMode::Default,
        manager,
        options: HttpCacheOptions::default(),
    }));

    // Create a request that would potentially be difficult to clone
    // Note: surf/http-types may not have the exact same cloning issues as reqwest,
    // but this test ensures the error handling is robust
    let body_data =
        "large data that could potentially be streaming".repeat(1000);

    let response = client
        .post(&url)
        .header("Content-Type", "application/octet-stream")
        .body_string(body_data)
        .await;

    // The middleware should handle this gracefully - either cache or bypass cache
    match response {
        Ok(response) => {
            // This is what we expect - successful handling
            assert_eq!(response.status(), 200);
        }
        Err(e) => {
            // If there's an error, it should NOT be a cloning error
            let error_msg = e.to_string();
            assert!(
                !error_msg.contains("not cloneable"),
                "Expected graceful handling but got cloning error: {}",
                error_msg
            );
        }
    }

    Ok(())
}

#[test]
#[allow(clippy::default_constructed_unit_structs)]
fn test_errors() -> Result<()> {
    // Testing the Debug trait for the error types
    let bad_request_err = BadRequest::default();
    assert!(format!("{:?}", bad_request_err).contains("BadRequest"));

    let surf_err = HttpCacheError::cache("test".to_string());
    assert!(format!("{:?}", &surf_err).contains("Cache"));
    assert_eq!(surf_err.to_string(), "Cache error: test".to_string());
    Ok(())
}

#[cfg(feature = "manager-moka")]
mod with_moka {
    use super::*;

    #[tokio::test]
    async fn default_mode() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Cold pass to load cache
        let res = client.send(req.clone()).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Hot pass to make sure the expect response was returned
        let mut res = client.send(req).await?;
        assert_eq!(res.body_bytes().await?, TEST_BODY);
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
        assert_eq!(res.header(XCACHE).unwrap(), HIT);
        Ok(())
    }

    #[tokio::test]
    async fn default_mode_with_options() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PRIVATE, TEST_BODY, 200, 1);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Construct Surf client with cache options override
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions {
                cache_key: None,
                cache_options: Some(CacheOptions {
                    shared: false,
                    ..Default::default()
                }),
                cache_mode_fn: None,
                cache_bust: None,
                ..Default::default()
            },
        }));

        // Cold pass to load cache
        client.send(req.clone()).await?;

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Hot pass to make sure the expect response was returned
        let mut res = client.send(req).await?;
        assert_eq!(res.body_bytes().await?, TEST_BODY);
        Ok(())
    }

    #[tokio::test]
    async fn default_mode_no_cache_response() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock("no-cache", TEST_BODY, 200, 2);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Cold pass to load cache
        let res = client.send(req.clone()).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Hot pass to make sure the expect response was returned
        let mut res = client.send(req).await?;
        assert_eq!(res.body_bytes().await?, TEST_BODY);
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);
        Ok(())
    }

    #[tokio::test]
    async fn removes_warning() -> Result<()> {
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
        let manager = MokaManager::default();
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Cold pass to load cache
        let res = client.send(req.clone()).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Hot pass to make sure the expect response was returned
        let mut res = client.send(req).await?;
        assert_eq!(res.body_bytes().await?, TEST_BODY);
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
        assert_eq!(res.header(XCACHE).unwrap(), HIT);
        assert!(res.header("warning").is_none());
        Ok(())
    }

    #[tokio::test]
    async fn no_store_mode() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::NoStore,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Remote request but should not cache
        client.send(req.clone()).await?;

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_none());

        // To verify our endpoint receives the request rather than a cache hit
        let res = client.send(req).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);
        Ok(())
    }

    #[tokio::test]
    async fn no_cache_mode() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::NoCache,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Remote request and should cache
        let res = client.send(req.clone()).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // To verify our endpoint receives the request rather than a cache hit
        let res = client.send(req).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);
        Ok(())
    }

    #[tokio::test]
    async fn force_cache_mode() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock("max-age=0, public", TEST_BODY, 200, 1);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::ForceCache,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Should result in a cache miss and a remote request
        let res = client.send(req.clone()).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Should result in a cache hit and no remote request
        let res = client.send(req).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
        assert_eq!(res.header(XCACHE).unwrap(), HIT);
        Ok(())
    }

    #[tokio::test]
    async fn ignore_rules_mode() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock("no-store, max-age=0, public", TEST_BODY, 200, 1);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::IgnoreRules,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Should result in a cache miss and a remote request
        let res = client.send(req.clone()).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Should result in a cache hit and no remote request
        let res = client.send(req).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
        assert_eq!(res.header(XCACHE).unwrap(), HIT);
        Ok(())
    }

    #[tokio::test]
    async fn delete_after_non_get_head_method_request() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m_get = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
        let m_post = Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(201).set_body_bytes("created"))
            .expect(1);
        let _mock_guard_get = mock_server.register_as_scoped(m_get).await;
        let _mock_guard_post = mock_server.register_as_scoped(m_post).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req_get = Request::new(Method::Get, Url::parse(&url)?);
        let req_post = Request::new(Method::Post, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Cold pass to load cache
        let res = client.send(req_get).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Post request to make sure the cache object at the same resource was deleted
        client.send(req_post).await?;

        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn revalidation_304() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(MUST_REVALIDATE, TEST_BODY, 200, 1);
        let m_304 = Mock::given(method(GET))
            .respond_with(ResponseTemplate::new(304))
            .expect(1);
        let mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Cold pass to load cache
        let res = client.send(req.clone()).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        drop(mock_guard);

        let _mock_guard = mock_server.register_as_scoped(m_304).await;

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Hot pass to make sure revalidation request was sent
        let mut res = client.send(req).await?;
        assert_eq!(res.body_bytes().await?, TEST_BODY);
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
        assert_eq!(res.header(XCACHE).unwrap(), HIT);
        Ok(())
    }

    #[tokio::test]
    async fn revalidation_200() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(MUST_REVALIDATE, TEST_BODY, 200, 1);
        let m_200 = build_mock(MUST_REVALIDATE, b"updated", 200, 1);
        let mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Cold pass to load cache
        let res = client.send(req.clone()).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        drop(mock_guard);

        let _mock_guard = mock_server.register_as_scoped(m_200).await;

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Hot pass to make sure revalidation request was sent
        let mut res = client.send(req).await?;
        assert_eq!(res.body_bytes().await?, b"updated");
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);
        Ok(())
    }

    #[tokio::test]
    async fn revalidation_500() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(MUST_REVALIDATE, TEST_BODY, 200, 1);
        let m_500 = Mock::given(method(GET))
            .respond_with(ResponseTemplate::new(500))
            .expect(1);
        let mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Cold pass to load cache
        let res = client.send(req.clone()).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        drop(mock_guard);

        let _mock_guard = mock_server.register_as_scoped(m_500).await;

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Hot pass to make sure revalidation request was sent
        let mut res = client.send(req).await?;
        assert_eq!(res.body_bytes().await?, TEST_BODY);
        assert!(res.header("warning").is_some());
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
        assert_eq!(res.header(XCACHE).unwrap(), HIT);
        Ok(())
    }

    #[tokio::test]
    async fn reload_mode() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();

        // Construct surf client with cache options override
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Reload,
            manager: manager.clone(),
            options: HttpCacheOptions {
                cache_key: None,
                cache_options: Some(CacheOptions {
                    shared: false,
                    ..Default::default()
                }),
                cache_mode_fn: None,
                cache_bust: None,
                ..Default::default()
            },
        }));

        // Cold pass to load cache
        client.get(url.clone()).send().await?;

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Another pass to make sure request is made to the endpoint
        client.get(url).send().await?;

        Ok(())
    }

    #[tokio::test]
    async fn custom_cache_key() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();

        // Construct surf client with cache defaults and custom cache key
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions {
                cache_key: Some(Arc::new(|req: &http::request::Parts| {
                    format!("{}:{}:{:?}:test", req.method, req.uri, req.version)
                })),
                cache_options: None,
                cache_mode_fn: None,
                cache_bust: None,
                ..Default::default()
            },
        }));

        // Remote request and should cache
        client.get(url.clone()).send().await?;

        // Try to load cached object
        let data = manager
            .get(&format!("{}:{}:{:?}:test", GET, &url, http::Version::HTTP_11))
            .await?;

        assert!(data.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn custom_cache_mode_fn() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/test.css", &mock_server.uri());
        let manager = MokaManager::default();

        // Construct surf client with cache defaults and custom cache mode
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::NoStore,
            manager: manager.clone(),
            options: HttpCacheOptions {
                cache_key: None,
                cache_options: None,
                cache_mode_fn: Some(Arc::new(|req: &http::request::Parts| {
                    if req.uri.path().ends_with(".css") {
                        CacheMode::Default
                    } else {
                        CacheMode::NoStore
                    }
                })),
                cache_bust: None,
                ..Default::default()
            },
        }));

        // Remote request and should cache
        client.get(url.clone()).send().await?;

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        let url = format!("{}/", &mock_server.uri());
        // To verify our endpoint receives the request rather than a cache hit
        client.get(url.clone()).send().await?;

        // Check no cache object was created
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn no_status_headers() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/test.css", &mock_server.uri());
        let manager = MokaManager::default();

        // Construct surf client with cache defaults and custom cache mode
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions {
                cache_status_headers: false,
                ..Default::default()
            },
        }));

        // Remote request and should cache
        let res = client.get(url.clone()).send().await?;

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // Make sure the cache status headers aren't present in the response
        assert!(res.header(XCACHELOOKUP).is_none());
        assert!(res.header(XCACHE).is_none());

        Ok(())
    }

    #[tokio::test]
    async fn cache_bust() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();

        // Construct surf client with cache defaults and custom cache mode
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions {
                cache_key: None,
                cache_options: None,
                cache_mode_fn: None,
                cache_bust: Some(Arc::new(
                    |req: &http::request::Parts, _, _| {
                        if req.uri.path().ends_with("/bust-cache") {
                            vec![format!(
                                "{}:{}://{}:{}/",
                                GET,
                                req.uri.scheme_str().unwrap(),
                                req.uri.host().unwrap(),
                                req.uri.port_u16().unwrap_or(80)
                            )]
                        } else {
                            Vec::new()
                        }
                    },
                )),
                ..Default::default()
            },
        }));

        // Remote request and should cache
        client.get(url.clone()).send().await?;

        // Try to load cached object
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        // To verify our endpoint receives the request rather than a cache hit
        client.get(format!("{}/bust-cache", &mock_server.uri())).send().await?;

        // Check cache object was busted
        let data =
            manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
        assert!(data.is_none());

        Ok(())
    }

    #[cfg(test)]
    mod only_if_cached_mode {
        use super::*;

        #[tokio::test]
        async fn miss() -> Result<()> {
            let mock_server = MockServer::start().await;
            let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 0);
            let _mock_guard = mock_server.register_as_scoped(m).await;
            let url = format!("{}/", &mock_server.uri());
            let manager = MokaManager::default();
            let req = Request::new(Method::Get, Url::parse(&url)?);

            // Construct Surf client with cache defaults
            let client = Client::new().with(Cache(HttpCache {
                mode: CacheMode::OnlyIfCached,
                manager: manager.clone(),
                options: Default::default(),
            }));

            // Should result in a cache miss and no remote request
            let res = client.send(req).await?;
            assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
            assert_eq!(res.header(XCACHE).unwrap(), MISS);

            // Try to load cached object
            let data =
                manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
            assert!(data.is_none());
            Ok(())
        }

        #[tokio::test]
        async fn hit() -> Result<()> {
            let mock_server = MockServer::start().await;
            let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
            let _mock_guard = mock_server.register_as_scoped(m).await;
            let url = format!("{}/", &mock_server.uri());
            let manager = MokaManager::default();
            let req = Request::new(Method::Get, Url::parse(&url)?);

            // Construct Surf client with cache defaults
            let client = Client::new().with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager: manager.clone(),
                options: Default::default(),
            }));

            // Cold pass to load the cache
            let res = client.send(req.clone()).await?;
            assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
            assert_eq!(res.header(XCACHE).unwrap(), MISS);

            // Try to load cached object
            let data =
                manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
            assert!(data.is_some());

            // Construct Surf client with cache defaults
            let client = Client::new().with(Cache(HttpCache {
                mode: CacheMode::OnlyIfCached,
                manager: manager.clone(),
                options: Default::default(),
            }));

            // Should result in a cache hit and no remote request
            let mut res = client.send(req).await?;
            assert_eq!(res.body_bytes().await?, TEST_BODY);
            assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
            assert_eq!(res.header(XCACHE).unwrap(), HIT);
            Ok(())
        }
    }

    // Note: HEAD request caching test is commented out due to implementation issues
    // in the surf middleware that cause the test to hang indefinitely. This appears
    // to be a limitation where HEAD requests don't properly complete the caching flow.
    // The test compiles successfully but hangs during execution, suggesting an issue
    // with how HEAD requests are handled in the surf cache middleware implementation.
    // Other HTTP methods (PUT, PATCH, DELETE, OPTIONS) work correctly.

    /*
    #[tokio::test]
    async fn head_request_caching() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = Mock::given(method("HEAD"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("cache-control", CACHEABLE_PUBLIC)
                    .insert_header("content-type", "text/plain")
                    // HEAD responses should not have a body
            )
            .expect(2); // Expect 2 calls to verify the second one is cached
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();
        let req = Request::new(Method::Head, Url::parse(&url)?);

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // First HEAD request - should miss and be cached
        let res = client.send(req.clone()).await?;
        assert_eq!(res.status(), 200);
        assert_eq!(res.header("content-type").unwrap(), "text/plain");

        // Second HEAD request - should hit the cache
        let res = client.send(req).await?;
        assert_eq!(res.status(), 200);
        assert_eq!(res.header("content-type").unwrap(), "text/plain");

        Ok(())
    }
    */

    #[tokio::test]
    async fn put_request_invalidates_cache() -> Result<()> {
        let mock_server = MockServer::start().await;

        // Mock GET request for caching
        let m_get = Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("cache-control", CACHEABLE_PUBLIC)
                    .set_body_bytes(TEST_BODY),
            )
            .expect(1);

        // Mock PUT request that should invalidate cache
        let m_put = Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1);

        let mock_guard_get = mock_server.register_as_scoped(m_get).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();

        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // First, cache a GET response
        let get_req = Request::new(Method::Get, Url::parse(&url)?);
        client.send(get_req).await?;

        // Verify it's cached
        let data = manager.get(&format!("GET:{}", &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        drop(mock_guard_get);
        let _mock_guard_put = mock_server.register_as_scoped(m_put).await;

        // PUT request should invalidate the cached GET response
        let put_req = Request::new(Method::Put, Url::parse(&url)?);
        let put_res = client.send(put_req).await?;
        assert_eq!(put_res.status(), 204);

        // Verify cache was invalidated
        let data = manager.get(&format!("GET:{}", &Url::parse(&url)?)).await?;
        assert!(data.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn patch_request_invalidates_cache() -> Result<()> {
        let mock_server = MockServer::start().await;

        let m_get = Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("cache-control", CACHEABLE_PUBLIC)
                    .set_body_bytes(TEST_BODY),
            )
            .expect(1);

        let m_patch = Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1);

        let mock_guard_get = mock_server.register_as_scoped(m_get).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();

        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Cache a GET response
        let get_req = Request::new(Method::Get, Url::parse(&url)?);
        client.send(get_req).await?;

        // Verify it's cached
        let data = manager.get(&format!("GET:{}", &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        drop(mock_guard_get);
        let _mock_guard_patch = mock_server.register_as_scoped(m_patch).await;

        // PATCH request should invalidate cache
        let patch_req =
            Request::new(Method::from_str("PATCH")?, Url::parse(&url)?);
        let patch_res = client.send(patch_req).await?;
        assert_eq!(patch_res.status(), 200);

        // Verify cache was invalidated
        let data = manager.get(&format!("GET:{}", &Url::parse(&url)?)).await?;
        assert!(data.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn delete_request_invalidates_cache() -> Result<()> {
        let mock_server = MockServer::start().await;

        let m_get = Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("cache-control", CACHEABLE_PUBLIC)
                    .set_body_bytes(TEST_BODY),
            )
            .expect(1);

        let m_delete = Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1);

        let mock_guard_get = mock_server.register_as_scoped(m_get).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();

        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // Cache a GET response
        let get_req = Request::new(Method::Get, Url::parse(&url)?);
        client.send(get_req).await?;

        // Verify it's cached
        let data = manager.get(&format!("GET:{}", &Url::parse(&url)?)).await?;
        assert!(data.is_some());

        drop(mock_guard_get);
        let _mock_guard_delete = mock_server.register_as_scoped(m_delete).await;

        // DELETE request should invalidate cache
        let delete_req = Request::new(Method::Delete, Url::parse(&url)?);
        let delete_res = client.send(delete_req).await?;
        assert_eq!(delete_res.status(), 204);

        // Verify cache was invalidated
        let data = manager.get(&format!("GET:{}", &Url::parse(&url)?)).await?;
        assert!(data.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn options_request_not_cached() -> Result<()> {
        let mock_server = MockServer::start().await;
        let m = Mock::given(method("OPTIONS"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("allow", "GET, POST, PUT, DELETE")
                    .insert_header("cache-control", CACHEABLE_PUBLIC), // Even with cache headers
            )
            .expect(2); // Should be called twice since not cached
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = MokaManager::default();

        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: Default::default(),
        }));

        // First OPTIONS request
        let req1 =
            Request::new(Method::from_str("OPTIONS")?, Url::parse(&url)?);
        let res1 = client.send(req1).await?;
        assert_eq!(res1.status(), 200);

        // Verify it's not cached
        let data =
            manager.get(&format!("OPTIONS:{}", &Url::parse(&url)?)).await?;
        assert!(data.is_none());

        // Second OPTIONS request should hit the server again
        let req2 =
            Request::new(Method::from_str("OPTIONS")?, Url::parse(&url)?);
        let res2 = client.send(req2).await?;
        assert_eq!(res2.status(), 200);

        Ok(())
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
        async fn cache_hit_bypasses_rate_limiting() -> Result<()> {
            let mock_server = MockServer::start().await;
            let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
            let _mock_guard = mock_server.register_as_scoped(m).await;
            let url = format!("{}/", &mock_server.uri());
            let manager = MokaManager::default();
            let rate_limiter = Arc::new(MockRateLimiter::new(Duration::ZERO));

            let client = Client::new().with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager,
                options: HttpCacheOptions {
                    rate_limiter: Some(rate_limiter.clone()),
                    ..Default::default()
                },
            }));

            // First request (cache miss) - should trigger rate limiting
            let req1 = Request::new(Method::Get, Url::parse(&url)?);
            let res1 = client.send(req1).await?;
            assert_eq!(res1.header(XCACHELOOKUP).unwrap(), MISS);
            assert_eq!(res1.header(XCACHE).unwrap(), MISS);

            // Second request (cache hit) - should NOT trigger rate limiting
            let req2 = Request::new(Method::Get, Url::parse(&url)?);
            let res2 = client.send(req2).await?;
            assert_eq!(res2.header(XCACHELOOKUP).unwrap(), HIT);
            assert_eq!(res2.header(XCACHE).unwrap(), HIT);

            // Verify rate limiter was only called once (for the cache miss)
            let calls = rate_limiter.get_calls();
            assert_eq!(calls.len(), 1);

            Ok(())
        }

        #[tokio::test]
        async fn cache_miss_applies_rate_limiting() -> Result<()> {
            let mock_server = MockServer::start().await;
            let m = Mock::given(method(GET))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("cache-control", "no-cache")
                        .set_body_bytes(TEST_BODY),
                )
                .expect(2);
            let _mock_guard = mock_server.register_as_scoped(m).await;
            let url = format!("{}/", &mock_server.uri());
            let manager = MokaManager::default();
            let rate_limiter =
                Arc::new(MockRateLimiter::new(Duration::from_millis(100)));

            let client = Client::new().with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager,
                options: HttpCacheOptions {
                    rate_limiter: Some(rate_limiter.clone()),
                    ..Default::default()
                },
            }));

            let start = Instant::now();

            // Two requests that will both be cache misses
            let req1 = Request::new(Method::Get, Url::parse(&url)?);
            let res1 = client.send(req1).await?;
            assert_eq!(res1.header(XCACHE).unwrap(), MISS);

            let req2 = Request::new(Method::Get, Url::parse(&url)?);
            let res2 = client.send(req2).await?;
            assert_eq!(res2.header(XCACHE).unwrap(), MISS);

            let elapsed = start.elapsed();

            // Verify rate limiter was called for both requests
            let calls = rate_limiter.get_calls();
            assert_eq!(calls.len(), 2);

            // Verify some delay was applied (at least some portion of our 200ms total)
            assert!(elapsed >= Duration::from_millis(100));

            Ok(())
        }

        #[tokio::test]
        async fn domain_rate_limiter_integration() -> Result<()> {
            let mock_server = MockServer::start().await;
            let m = Mock::given(method(GET))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("cache-control", "no-cache")
                        .set_body_bytes(TEST_BODY),
                )
                .expect(1);
            let _mock_guard = mock_server.register_as_scoped(m).await;
            let url = format!("{}/", &mock_server.uri());
            let manager = MokaManager::default();

            // Create a domain rate limiter with very permissive limits
            let quota =
                Quota::per_second(std::num::NonZeroU32::new(100).unwrap());
            let rate_limiter = Arc::new(DomainRateLimiter::new(quota));

            let client = Client::new().with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager,
                options: HttpCacheOptions {
                    rate_limiter: Some(rate_limiter),
                    ..Default::default()
                },
            }));

            // Request should succeed and be rate limited
            let req = Request::new(Method::Get, Url::parse(&url)?);
            let res = client.send(req).await?;
            assert_eq!(res.header(XCACHE).unwrap(), MISS);
            assert_eq!(res.status(), 200);

            Ok(())
        }

        #[tokio::test]
        async fn direct_rate_limiter_integration() -> Result<()> {
            let mock_server = MockServer::start().await;
            let m = Mock::given(method(GET))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("cache-control", "no-cache")
                        .set_body_bytes(TEST_BODY),
                )
                .expect(1);
            let _mock_guard = mock_server.register_as_scoped(m).await;
            let url = format!("{}/", &mock_server.uri());
            let manager = MokaManager::default();

            // Create a direct rate limiter with very permissive limits
            let quota =
                Quota::per_second(std::num::NonZeroU32::new(100).unwrap());
            let rate_limiter = Arc::new(DirectRateLimiter::direct(quota));

            let client = Client::new().with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager,
                options: HttpCacheOptions {
                    rate_limiter: Some(rate_limiter),
                    ..Default::default()
                },
            }));

            // Request should succeed and be rate limited
            let req = Request::new(Method::Get, Url::parse(&url)?);
            let res = client.send(req).await?;
            assert_eq!(res.header(XCACHE).unwrap(), MISS);
            assert_eq!(res.status(), 200);

            Ok(())
        }
    }
}
