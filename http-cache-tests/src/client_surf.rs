use crate::*;
use std::sync::Arc;

use http_cache_surf::Cache;

use surf::{middleware::Next, Client, Request};

#[async_std::test]
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
        options: None,
    }));

    // Cold pass to load cache
    let res = client.send(req.clone()).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_some());

    // Hot pass to make sure the expect response was returned
    let mut res = client.send(req).await?;
    assert_eq!(res.body_bytes().await?, TEST_BODY);
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
    assert_eq!(res.header(XCACHE).unwrap(), HIT);
    Ok(())
}

#[async_std::test]
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
        options: Some(CacheOptions { shared: false, ..Default::default() }),
    }));

    // Cold pass to load cache
    client.send(req.clone()).await?;

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_some());

    // Hot pass to make sure the expect response was returned
    let mut res = client.send(req).await?;
    assert_eq!(res.body_bytes().await?, TEST_BODY);
    Ok(())
}

#[async_std::test]
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
        options: None,
    }));

    // Cold pass to load cache
    let res = client.send(req.clone()).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_some());

    // Hot pass to make sure the expect response was returned
    let mut res = client.send(req).await?;
    assert_eq!(res.body_bytes().await?, TEST_BODY);
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);
    Ok(())
}

#[async_std::test]
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
        options: None,
    }));

    // Cold pass to load cache
    let res = client.send(req.clone()).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_some());

    // Hot pass to make sure the expect response was returned
    let mut res = client.send(req).await?;
    assert_eq!(res.body_bytes().await?, TEST_BODY);
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
    assert_eq!(res.header(XCACHE).unwrap(), HIT);
    assert!(res.header("warning").is_none());
    Ok(())
}

#[async_std::test]
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
        options: None,
    }));

    // Remote request but should not cache
    client.send(req.clone()).await?;

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_none());

    // To verify our endpoint receives the request rather than a cache hit
    let res = client.send(req).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);
    Ok(())
}

#[async_std::test]
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
        options: None,
    }));

    // Remote request and should cache
    let res = client.send(req.clone()).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_some());

    // To verify our endpoint receives the request rather than a cache hit
    let res = client.send(req).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);
    Ok(())
}

#[async_std::test]
async fn force_cache_mode() -> Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = MokaManager::default();
    let req = Request::new(Method::Get, Url::parse(&url)?);

    // Construct Surf client with cache defaults
    let client = Client::new().with(Cache(HttpCache {
        mode: CacheMode::ForceCache,
        manager: manager.clone(),
        options: None,
    }));

    // Should result in a cache miss and a remote request
    let res = client.send(req.clone()).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_some());

    // Should result in a cache hit and no remote request
    let res = client.send(req).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
    assert_eq!(res.header(XCACHE).unwrap(), HIT);
    Ok(())
}

#[async_std::test]
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
        options: None,
    }));

    // Cold pass to load cache
    let res = client.send(req_get).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_some());

    // Post request to make sure the cache object at the same resource was deleted
    client.send(req_post).await?;

    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_none());

    Ok(())
}

#[async_std::test]
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
        options: None,
    }));

    // Cold pass to load cache
    let res = client.send(req.clone()).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);

    drop(mock_guard);

    let _mock_guard = mock_server.register_as_scoped(m_304).await;

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_some());

    // Hot pass to make sure revalidation request was sent
    let mut res = client.send(req).await?;
    assert_eq!(res.body_bytes().await?, TEST_BODY);
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
    assert_eq!(res.header(XCACHE).unwrap(), HIT);
    Ok(())
}

#[async_std::test]
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
        options: None,
    }));

    // Cold pass to load cache
    let res = client.send(req.clone()).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);

    drop(mock_guard);

    let _mock_guard = mock_server.register_as_scoped(m_200).await;

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_some());

    // Hot pass to make sure revalidation request was sent
    let mut res = client.send(req).await?;
    assert_eq!(res.body_bytes().await?, b"updated");
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);
    Ok(())
}

#[async_std::test]
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
        options: None,
    }));

    // Cold pass to load cache
    let res = client.send(req.clone()).await?;
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
    assert_eq!(res.header(XCACHE).unwrap(), MISS);

    drop(mock_guard);

    let _mock_guard = mock_server.register_as_scoped(m_500).await;

    // Try to load cached object
    let data = manager.get(GET, &Url::parse(&url)?).await?;
    assert!(data.is_some());

    // Hot pass to make sure revalidation request was sent
    let mut res = client.send(req).await?;
    assert_eq!(res.body_bytes().await?, TEST_BODY);
    assert!(res.header("warning").is_some());
    assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
    assert_eq!(res.header(XCACHE).unwrap(), HIT);
    Ok(())
}

#[cfg(test)]
mod only_if_cached_mode {
    use super::*;

    #[async_std::test]
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
            options: None,
        }));

        // Should result in a cache miss and no remote request
        let res = client.send(req).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        // Try to load cached object
        let data = manager.get(GET, &Url::parse(&url)?).await?;
        assert!(data.is_none());
        Ok(())
    }

    #[async_std::test]
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
            options: None,
        }));

        // Cold pass to load the cache
        let res = client.send(req.clone()).await?;
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), MISS);
        assert_eq!(res.header(XCACHE).unwrap(), MISS);

        // Try to load cached object
        let data = manager.get(GET, &Url::parse(&url)?).await?;
        assert!(data.is_some());

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::OnlyIfCached,
            manager: manager.clone(),
            options: None,
        }));

        // Should result in a cache hit and no remote request
        let mut res = client.send(req).await?;
        assert_eq!(res.body_bytes().await?, TEST_BODY);
        assert_eq!(res.header(XCACHELOOKUP).unwrap(), HIT);
        assert_eq!(res.header(XCACHE).unwrap(), HIT);
        Ok(())
    }
}
