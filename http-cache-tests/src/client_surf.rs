use crate::*;

use http_cache_surf::Cache;

use surf::{middleware::Next, Client, Request};

#[async_std::test]
async fn default_mode() -> surf::Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = CACacheManager::default();
    let path = manager.path.clone();
    let key = format!("{}:{}", GET, &url);
    let req = Request::new(Method::Get, Url::parse(&url)?);

    // Make sure the record doesn't already exist
    manager.delete(GET, &Url::parse(&url)?).await?;

    // Construct Surf client with cache defaults
    let client = Client::new().with(Cache(HttpCache {
        mode: CacheMode::Default,
        manager: CACacheManager::default(),
        options: None,
    }));

    // Cold pass to load cache
    client.send(req.clone()).await?;

    // Try to load cached object
    let data = cacache::read(&path, &key).await;
    assert!(data.is_ok());

    // Hot pass to make sure the expect response was returned
    let mut res = client.send(req).await?;
    assert_eq!(res.body_bytes().await?, TEST_BODY);
    Ok(())
}

#[async_std::test]
async fn default_mode_with_options() -> surf::Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PRIVATE, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = CACacheManager::default();
    let path = manager.path.clone();
    let key = format!("{}:{}", GET, &url);
    let req = Request::new(Method::Get, Url::parse(&url)?);

    // Make sure the record doesn't already exist
    manager.delete(GET, &Url::parse(&url)?).await?;

    // Construct Surf client with cache options override
    let client = Client::new().with(Cache(HttpCache {
        mode: CacheMode::Default,
        manager: CACacheManager::default(),
        options: Some(CacheOptions { shared: false, ..Default::default() }),
    }));

    // Cold pass to load cache
    client.send(req.clone()).await?;

    // Try to load cached object
    let data = cacache::read(&path, &key).await;
    assert!(data.is_ok());

    // Hot pass to make sure the expect response was returned
    let mut res = client.send(req).await?;
    assert_eq!(res.body_bytes().await?, TEST_BODY);
    Ok(())
}

#[async_std::test]
async fn no_store_mode() -> surf::Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = CACacheManager::default();
    let path = manager.path.clone();
    let key = format!("{}:{}", GET, &url);
    let req = Request::new(Method::Get, Url::parse(&url)?);

    // Make sure the record doesn't already exist
    manager.delete(GET, &Url::parse(&url)?).await?;

    // Construct Surf client with cache defaults
    let client = Client::new().with(Cache(HttpCache {
        mode: CacheMode::NoStore,
        manager: CACacheManager::default(),
        options: None,
    }));

    // Remote request but should not cache
    client.send(req.clone()).await?;

    // Try to load cached object
    let data = cacache::read(&path, &key).await;
    assert!(data.is_err());

    // To verify our endpoint receives the request rather than a cache hit
    client.send(req.clone()).await?;
    Ok(())
}

#[async_std::test]
async fn no_cache_mode() -> surf::Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = CACacheManager::default();
    let path = manager.path.clone();
    let key = format!("{}:{}", GET, &url);
    let req = Request::new(Method::Get, Url::parse(&url)?);

    // Make sure the record doesn't already exist
    manager.delete(GET, &Url::parse(&url)?).await?;

    // Construct Surf client with cache defaults
    let client = Client::new().with(Cache(HttpCache {
        mode: CacheMode::NoCache,
        manager: CACacheManager::default(),
        options: None,
    }));

    // Remote request and should cache
    client.send(req.clone()).await?;

    // Try to load cached object
    let data = cacache::read(&path, &key).await;
    assert!(data.is_ok());

    // To verify our endpoint receives the request rather than a cache hit
    client.send(req.clone()).await?;
    Ok(())
}

#[async_std::test]
async fn force_cache_mode() -> surf::Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = CACacheManager::default();
    let path = manager.path.clone();
    let key = format!("{}:{}", GET, &url);
    let req = Request::new(Method::Get, Url::parse(&url)?);

    // Make sure the record doesn't already exist
    manager.delete(GET, &Url::parse(&url)?).await?;

    // Construct Surf client with cache defaults
    let client = Client::new().with(Cache(HttpCache {
        mode: CacheMode::ForceCache,
        manager: CACacheManager::default(),
        options: None,
    }));

    // Should result in a cache miss and a remote request
    client.send(req.clone()).await?;

    // Try to load cached object
    let data = cacache::read(&path, &key).await;
    assert!(data.is_ok());

    // Should result in a cache hit and no remote request
    client.send(req.clone()).await?;
    Ok(())
}

#[cfg(test)]
mod only_if_cached_mode {
    use super::*;

    #[async_std::test]
    async fn miss() -> surf::Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 0);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = CACacheManager::default();
        let path = manager.path.clone();
        let key = format!("{}:{}", GET, &url);
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Make sure the record doesn't already exist
        manager.delete(GET, &Url::parse(&url)?).await?;

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::OnlyIfCached,
            manager: CACacheManager::default(),
            options: None,
        }));

        // Should result in a cache miss and no remote request
        client.send(req.clone()).await?;

        // Try to load cached object
        let data = cacache::read(&path, &key).await;
        assert!(data.is_err());
        Ok(())
    }

    #[async_std::test]
    async fn hit() -> surf::Result<()> {
        let mock_server = MockServer::start().await;
        let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
        let _mock_guard = mock_server.register_as_scoped(m).await;
        let url = format!("{}/", &mock_server.uri());
        let manager = CACacheManager::default();
        let path = manager.path.clone();
        let key = format!("{}:{}", GET, &url);
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Make sure the record doesn't already exist
        manager.delete(GET, &Url::parse(&url)?).await?;

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager::default(),
            options: None,
        }));

        // Cold pass to load the cache
        client.send(req.clone()).await?;

        // Try to load cached object
        let data = cacache::read(&path, &key).await;
        assert!(data.is_ok());

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::OnlyIfCached,
            manager: CACacheManager::default(),
            options: None,
        }));

        // Should result in a cache hit and no remote request
        let mut res = client.send(req.clone()).await?;

        // Check the body
        assert_eq!(res.body_bytes().await?, TEST_BODY);
        Ok(())
    }
}
