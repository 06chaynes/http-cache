use crate::*;

use http_cache_reqwest::Cache;
use reqwest::{Client, Request, ResponseBuilderExt};
use reqwest_middleware::ClientBuilder;

#[tokio::test]
async fn default_mode() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = CACacheManager::default();
    let path = manager.path.clone();
    let key = format!("{}:{}", GET, &url);

    // Make sure the record doesn't already exist
    manager.delete(GET, &Url::parse(&url)?).await?;

    // Construct reqwest client with cache defaults
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager::default(),
            options: None,
        }))
        .build();

    // Cold pass to load cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = cacache::read(&path, &key).await;
    assert!(data.is_ok());

    // Hot pass to make sure the expect response was returned
    let res = client.get(url).send().await?;
    assert_eq!(res.bytes().await?, TEST_BODY);
    Ok(())
}

#[tokio::test]
async fn default_mode_with_options() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = CACacheManager::default();
    let path = manager.path.clone();
    let key = format!("{}:{}", GET, &url);

    // Make sure the record doesn't already exist
    manager.delete(GET, &Url::parse(&url)?).await?;

    // Construct reqwest client with cache options override
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager::default(),
            options: Some(CacheOptions { shared: false, ..Default::default() }),
        }))
        .build();

    // Cold pass to load cache
    client.get(url).send().await?;

    // Try to load cached object
    let data = cacache::read(&path, &key).await;
    assert!(data.is_ok());
    Ok(())
}

#[tokio::test]
async fn no_cache_mode() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = CACacheManager::default();
    let path = manager.path.clone();
    let key = format!("{}:{}", GET, &url);

    // Make sure the record doesn't already exist
    manager.delete(GET, &Url::parse(&url)?).await?;

    // Construct reqwest client with cache defaults
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::NoCache,
            manager: CACacheManager::default(),
            options: None,
        }))
        .build();

    // Remote request and should cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = cacache::read(&path, &key).await;
    assert!(data.is_ok());

    // To verify our endpoint receives the request rather than a cache hit
    client.get(url).send().await?;

    // Added to test clearing the cache
    manager.clear().await?;
    Ok(())
}
