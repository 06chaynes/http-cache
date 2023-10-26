use crate::MokaManager;
use std::sync::Arc;

use http_cache::*;
use http_cache_reqwest::Cache;
use http_cache_semantics::CachePolicy;
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

#[tokio::test]
async fn moka() -> Result<()> {
    // Added to test custom Debug impl
    assert_eq!(format!("{:?}", MokaManager::default()), "MokaManager { .. }",);
    let url = Url::parse("http://example.com")?;
    let manager = Arc::new(MokaManager::default());
    let http_res = HttpResponse {
        body: TEST_BODY.to_vec(),
        headers: Default::default(),
        status: 200,
        url: url.clone(),
        version: HttpVersion::Http11,
    };
    let req = http::Request::get("http://example.com").body(())?;
    let res = http::Response::builder().status(200).body(TEST_BODY.to_vec())?;
    let policy = CachePolicy::new(&req, &res);
    manager
        .put(format!("{}:{}", GET, &url), http_res.clone(), policy.clone())
        .await?;
    let data = manager.get(&format!("{}:{}", GET, &url)).await?;
    assert!(data.is_some());
    assert_eq!(data.unwrap().0.body, TEST_BODY);
    manager.delete(&format!("{}:{}", GET, &url)).await?;
    let data = manager.get(&format!("{}:{}", GET, &url)).await?;
    assert!(data.is_none());
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
                cache_bust: None,
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
