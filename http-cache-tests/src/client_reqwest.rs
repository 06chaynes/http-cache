use crate::*;
use std::sync::Arc;

use http_cache_reqwest::Cache;
use reqwest::{Client, Request, ResponseBuilderExt};
use reqwest_middleware::ClientBuilder;

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
            options: None,
        }))
        .build();

    // Cold pass to load cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
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
            options: Some(CacheOptions { shared: false, ..Default::default() }),
        }))
        .build();

    // Cold pass to load cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
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
            options: None,
        }))
        .build();

    // Remote request and should cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_some());

    // To verify our endpoint receives the request rather than a cache hit
    client.get(url).send().await?;
    Ok(())
}
