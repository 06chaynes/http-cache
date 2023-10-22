use crate::{error, Cache};
use std::sync::Arc;

use http_cache::*;
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use url::Url;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

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

#[test]
#[allow(clippy::default_constructed_unit_structs)]
fn test_errors() -> Result<()> {
    // Testing the Debug, Default, and Clone traits for the error types
    let br = error::BadRequest::default();
    assert_eq!(format!("{:?}", br.clone()), "BadRequest",);
    assert_eq!(
        br.to_string(),
        "Request object is not cloneable. Are you passing a streaming body?"
            .to_string(),
    );
    Ok(())
}

#[tokio::test]
async fn default_mode() -> Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = MokaManager::default();

    // Construct reqwest client with cache defaults
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        }))
        .build();

    // Cold pass to load cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
    assert!(data.is_some());

    // Hot pass to make sure the expect response was returned
    let res = client.get(url).send().await?;
    assert_eq!(res.bytes().await?, TEST_BODY);
    Ok(())
}

#[tokio::test]
async fn default_mode_with_options() -> Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = MokaManager::default();

    // Construct reqwest client with cache options override
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions {
                cache_key: None,
                cache_options: Some(CacheOptions {
                    shared: false,
                    ..Default::default()
                }),
                cache_mode_fn: None,
            },
        }))
        .build();

    // Cold pass to load cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
    assert!(data.is_some());
    Ok(())
}

#[tokio::test]
async fn no_cache_mode() -> Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = MokaManager::default();

    // Construct reqwest client with cache defaults
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::NoCache,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        }))
        .build();

    // Remote request and should cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
    assert!(data.is_some());

    // To verify our endpoint receives the request rather than a cache hit
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

    // Construct reqwest client with cache defaults and custom cache key
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions {
                cache_key: Some(Arc::new(|req: &http::request::Parts| {
                    format!("{}:{}:{:?}:test", req.method, req.uri, req.version)
                })),
                cache_options: None,
                cache_mode_fn: None,
            },
        }))
        .build();

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

    // Construct reqwest client with cache defaults and custom cache mode
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
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
            },
        }))
        .build();

    // Remote request and should cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
    assert!(data.is_some());

    let url = format!("{}/", &mock_server.uri());
    // To verify our endpoint receives the request rather than a cache hit
    client.get(url.clone()).send().await?;

    // Check no cache object was created
    let data = manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
    assert!(data.is_none());

    Ok(())
}

#[tokio::test]
async fn delete_after_non_get_head_method_request() -> Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = MokaManager::default();

    // Construct reqwest client with cache defaults
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        }))
        .build();

    // Cold pass to load cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
    assert!(data.is_some());

    // Post request to make sure the cache object at the same resource was deleted
    client.post(url.clone()).send().await?;

    let data = manager.get(&format!("{}:{}", GET, &Url::parse(&url)?)).await?;
    assert!(data.is_none());

    Ok(())
}
